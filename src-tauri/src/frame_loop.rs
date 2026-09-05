use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ai_buddy_core::director::{self, Context, Director, Happened, Wake};
use ai_buddy_core::engine::{Cue, State, Verb};
use ai_buddy_core::input::press_target;
use ai_buddy_core::overlay::{bubble_owner, display_index_for, place_sprite};
use ai_buddy_core::roster::{InstanceId, Roster};
use ai_buddy_core::sensing::{Activity, FreeTier, SystemClock};
use ai_buddy_core::snapshot::SnapshotAssembler;
use ai_buddy_core::tools::DenyList;
use ai_buddy_core::visibility::{fullscreen_frontmost, Change, Desktop, HideRules};
use ai_buddy_core::window_source::{Rect, WindowSource};
use tauri::{Emitter, Manager};

use super::settings::SettingsOp;
use super::{
    apply_menu_action, describe_menu, dev_flags, menu, model, overlay_label, place_overlays,
    platform, publish_instances, remember_instances, spawn_live, switch_instance, tray,
    DirectorRun, Drawn, FrameExtras, InstanceState, MenuChannel, MenuHold, MenuSignal, Placed,
    Placement, SpritePlacement, Traced, TrayHandle, ENGINE_TICK, FRAME_EVENT, MENU_HOLD_TIMEOUT,
    SENSE_INTERVAL,
};

/// One overlay's last applied shape: the mask, then x, y, width and height.
///
/// Named because the tuple is three types deep and clippy's `type_complexity`
/// rejects it inline. Only the X11 lane keeps one, since XShape is what has to
/// be spared a rebuild every tick.
#[cfg(all(unix, not(target_os = "macos")))]
type MaskParams = (Option<Vec<bool>>, i32, i32, i32, i32);

/// The frame loop: assemble a snapshot, tick the Engine, apply the `Frame`.
///
/// Applying a `Frame` is two things at once, which is why they share a loop.
/// The webview is told where to draw and the hit-test is told where the sprite
/// is, both out of one `Frame`, so the outline the hit-test measures belongs to
/// the Animation frame the user sees.
///
/// Position is where the two part. The webview draws one sample behind and
/// interpolates towards this one, so the hit-test rectangle leads what is on
/// screen by up to one tick. src/interpolate.js carries the measurements that
/// make that lag the cheaper half of the trade against a stuttering sprite.
// One over the clippy cap. Director config belongs here. Folding it into
// the other seven would mix a timer with window geometry.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_frame_loop(
    app: tauri::AppHandle,
    mut roster: Roster,
    mut lives: Vec<InstanceState>,
    source: impl WindowSource + Send + 'static,
    displays: platform::DisplayCache,
    rules: Arc<Mutex<HideRules>>,
    covered: Vec<Rect>,
    director_run: DirectorRun,
    menu_channel: MenuChannel,
    extras: FrameExtras,
) {
    let MenuChannel {
        sender: menu_sender,
        receiver: menu_signals,
    } = menu_channel;
    thread::spawn(move || {
        let mut assembler = SnapshotAssembler::new(source);
        let DirectorRun {
            mut config,
            settings: mut director,
            inspect,
        } = director_run;
        let FrameExtras {
            settings,
            settings_path,
            characters,
            instances: instance_rows,
            ops,
        } = extras;
        let mut slots = model::Slots::new();
        publish_instances(&roster, &instance_rows);
        let (mut tray_actions, mut last_menu) = {
            let installed: Vec<String> = characters.keys().cloned().collect();
            let current = lives
                .first()
                .map(|live| live.character.name.clone())
                .unwrap_or_default();
            let id = lives
                .first()
                .map(|live| live.id.clone())
                .unwrap_or_default();
            let settings_now = settings.lock().ok().map(|s| s.clone()).unwrap_or_default();
            let rules_now = rules.lock().ok();
            let description = describe_menu(
                &installed,
                &current,
                &roster,
                &id,
                &settings_now,
                rules_now.as_deref().unwrap_or(&HideRules::default()),
            );
            (description.actions.clone(), Some(description))
        };

        // Read once for every Instance: there is one desktop and one user, and
        // asking AppKit how long they have been idle once per buddy would be
        // the same answer bought several times.
        let mut free_tier = FreeTier::default();
        let activity_source = platform::activity_source();
        let mut since_sense = Duration::ZERO;
        let mut last_activity: Option<Activity> = None;

        // One click-through flag per overlay, `None` until that overlay's first
        // decision so the first tick always applies.
        let mut ignoring: Vec<Option<bool>> = vec![None; covered.len()];

        // Track whether each overlay's EWMH configuration (floating, skip taskbar)
        // succeeded. Retried on each frame until successful. GTK may not have a
        // window handle immediately after show(). Shared with main thread so
        // configure_overlay can report success.
        let configured = Arc::new(Mutex::new(vec![false; covered.len()]));

        // Track whether each overlay's XShape input mask has successfully applied.
        // On Linux/GTK, update_input_region needs the main thread AND a realized
        // window. Do not set ignore_cursor_events(false) until the mask succeeds,
        // or the entire overlay becomes a click-eater. Shared with main thread so
        // update_input_region can report success.
        let mask_applied = Arc::new(Mutex::new(vec![false; covered.len()]));

        // Track whether configure_overlay / update_input_region work is in flight
        // to avoid queuing redundant main-thread posts every 16ms.
        #[cfg(all(unix, not(target_os = "macos")))]
        let configure_in_flight = Arc::new(Mutex::new(vec![false; covered.len()]));
        #[cfg(all(unix, not(target_os = "macos")))]
        let mask_in_flight = Arc::new(Mutex::new(vec![false; covered.len()]));

        // Cache last applied mask parameters to avoid rebuilding the X pixmap
        // every 16ms. Only update XShape when mask data or position changes.
        // Shared with main thread so update_input_region can report success.
        #[cfg(all(unix, not(target_os = "macos")))]
        let last_mask: Arc<Mutex<Vec<MaskParams>>> =
            Arc::new(Mutex::new(vec![(None, 0, 0, 1, 1); covered.len()]));

        // The displays the overlays cover, as setup left them. Shared with the
        // main thread, which is the only place that can change what they cover
        // and so the only place that knows when this is true again.
        let covered = Arc::new(Mutex::new(covered));

        let mut button_was_down = false;
        let mut sound_allowed = true;
        let mut ticks: u32 = 0;
        let mut last_tick = Instant::now();

        loop {
            thread::sleep(ENGINE_TICK);

            // Read per tick, not once at setup: the Development tab can flip
            // these while the loop runs, and an atomic load is nothing beside
            // a frame. See `dev_flags`.
            //
            // Click-through is invisible: nothing on screen says whether the
            // overlay is currently swallowing clicks or passing them on. This
            // trace is the only way to watch the decision without a human
            // clicking. Off unless asked for; see scripts/verify-overlay.sh.
            let tracing = dev_flags::TRACE_HITTEST.is_on();

            // Likewise for the Frame: where the sprite is and what it is doing
            // is the loop's only output, and a screenshot cannot say whether it
            // got there by falling.
            let tracing_frames = dev_flags::TRACE_FRAMES.is_on();
            // And for what the Engine is playing. The frame line above says
            // which Animation is on screen but not what chose it: a `talk` is a
            // proposed Behavior, a cursor reaction and a Dwell alike.
            let tracing_engine = dev_flags::TRACE_ENGINE.is_on();
            // A click is two edges. The periodic hit-test line only prints on a
            // click-through flip or every two seconds, so a press that did not
            // flip — already over the sprite, or never over it — left no record
            // of whether the button was seen or the hit-test agreed.
            let tracing_director = model::tracing();
            let tracing_clicks = tracing || tracing_frames || tracing_director;

            let Ok(cursor) = app.cursor_position() else {
                continue;
            };

            // The windowing layer reports the global cursor against the primary
            // display's scale factor, whichever display it is actually over, so
            // that factor is what undoes it. It arrives from the cache rather
            // than from a monitor here: asking a monitor its scale means asking
            // `NSScreen`, and only the main thread may do that.
            let displays = displays.read();
            let cursor_scale = displays.cursor_scale;

            // One flag per overlay, and the desktop can gain or lose one.
            ignoring.resize(displays.frames.len(), None);
            configured
                .lock()
                .unwrap()
                .resize(displays.frames.len(), false);
            mask_applied
                .lock()
                .unwrap()
                .resize(displays.frames.len(), false);
            #[cfg(all(unix, not(target_os = "macos")))]
            configure_in_flight
                .lock()
                .unwrap()
                .resize(displays.frames.len(), false);
            #[cfg(all(unix, not(target_os = "macos")))]
            mask_in_flight
                .lock()
                .unwrap()
                .resize(displays.frames.len(), false);
            #[cfg(all(unix, not(target_os = "macos")))]
            last_mask
                .lock()
                .unwrap()
                .resize(displays.frames.len(), (None, 0, 0, 1, 1));

            // Wall time since the last tick that reached the Engine, not since
            // the last turn of this loop: a tick that could not read the
            // platform skips without advancing anything, and the time it spent
            // still passed for the sprite. `SnapshotAssembler` caps what it
            // hands the Engine, so a long gap — a skipped read, a suspended
            // process, a slept machine — is absorbed rather than slingshot.
            let elapsed_ms = u32::try_from(last_tick.elapsed().as_millis()).unwrap_or(u32::MAX);
            last_tick = Instant::now();

            // The Engine works in points across every display, which is the
            // space the cursor reading becomes once its own scale is undone.
            let cursor_points = ai_buddy_core::engine::Point {
                x: cursor.x / cursor_scale,
                y: cursor.y / cursor_scale,
            };

            // The hit-test asks its question in that shared space rather than
            // in an overlay's. Every overlay is handed the same sprite in its
            // own coordinates, so the answer is the same whichever overlay it
            // is asked of, and asking once is one answer instead of one per
            // window that could disagree.
            let cursor_at = (
                cursor_points.x.round() as i32,
                cursor_points.y.round() as i32,
            );

            // A dismissed Instance is gone from the Roster, and its Shell state
            // goes with it. Dropped before anything is hit-tested rather than
            // skipped inside the loop: a pointer left behind still counts as a
            // gesture, and one mid-drag when its Instance was dismissed would
            // hold every other buddy's presses for as long as the button stayed
            // down.
            lives.retain(|live| roster.get(&live.id).is_some());

            // Last tick's answer, which is the right one: the art being
            // hit-tested is the art that was last drawn. A Character nobody can
            // see is not there to be pressed, so a click where it would have
            // been reaches the window underneath and pokes nothing.
            let visible = rules.lock().is_ok_and(|rules| rules.presence().visible);

            let pressed: Vec<bool> = lives
                .iter()
                .map(|live| {
                    visible
                        && live.drawn_last.as_ref().is_some_and(|last| {
                            live.character
                                .draw(last.animation, last.animation_ms, last.variant_draw)
                                .is_some_and(|art| {
                                    art.mask.hit(
                                        &last.rect,
                                        cursor_at.0,
                                        cursor_at.1,
                                        last.mirrored,
                                    )
                                })
                        })
                })
                .collect();

            // One cursor, several sprites, and at most one gesture. Decided
            // across every Instance before any of them is told, because two
            // overlapping sprites handed the same hit-test would both be picked
            // up by one press.
            let gesturing = lives.iter().position(|live| live.pointer.gesturing());
            let target = press_target(&pressed, gesturing);

            // Last tick's click-through is the one that decided whether the
            // overlay could hear this press. Passing through means it cannot
            // still be holding a button, so a lost pointerup is dropped here
            // rather than gluing the sprite to a hand that has gone. The
            // session poll is not consulted: it is the one that misses a
            // press our own window swallowed, which is when this latch is
            // the only witness.
            let on_overlay =
                display_index_for((cursor_points.x, cursor_points.y), &displays.frames);
            if !visible {
                platform::overlay_passes_clicks_through();
            } else if let Some(index) = on_overlay {
                if ignoring.get(index).copied().flatten() == Some(true) {
                    platform::overlay_passes_clicks_through();
                }
            } else {
                platform::overlay_passes_clicks_through();
            }
            // A consuming read — once per tick, nowhere else. A second read
            // eats the edge (#182).
            let buttons = platform::buttons_down();
            let held = buttons.primary;
            let secondary_held = buttons.secondary;
            let button_edge = match (button_was_down, held) {
                (false, true) => Some("down"),
                (true, false) => Some("up"),
                _ => None,
            };
            button_was_down = held;
            if tracing_clicks {
                if let Some(edge) = button_edge {
                    let sprite = target
                        .and_then(|index| lives.get(index))
                        .and_then(|live| live.drawn_last.as_ref())
                        .map(|last| format!("({},{})", last.rect.x, last.rect.y))
                        .or_else(|| {
                            lives.first().and_then(|live| {
                                live.drawn_last.as_ref().map(|last| {
                                    format!("untargeted({},{})", last.rect.x, last.rect.y)
                                })
                            })
                        })
                        .unwrap_or_else(|| "none".to_string());
                    eprintln!(
                        "click: {edge} hits={pressed:?} target={target:?} \
                         cursor=({:.0},{:.0})->({},{}) scale={:.1} \
                         visible={visible} sprite={sprite}",
                        cursor.x, cursor.y, cursor_at.0, cursor_at.1, cursor_scale,
                    );
                }
            }

            // What the main thread has said about the open menu since the last
            // tick. Drained in full rather than one message a tick: a click and
            // the close that follows it arrive together, and taking one per
            // frame would leave the Instance held for a tick after the menu was
            // already gone.
            let mut chosen: Vec<String> = Vec::new();
            let mut menu_closed = false;
            loop {
                match menu_signals.try_recv() {
                    Ok(MenuSignal::Chose(id)) => chosen.push(id),
                    Ok(MenuSignal::Closed) => menu_closed = true,
                    Err(mpsc::TryRecvError::Empty) => break,
                    // Nobody left to pop a menu, so no menu can still be on
                    // screen. Ending the hold matters more than the reason: an
                    // Instance handed Verb::Menu every tick forever never moves
                    // again, and that outlives whatever went wrong.
                    Err(mpsc::TryRecvError::Disconnected) => {
                        menu_closed = true;
                        break;
                    }
                }
            }

            // Which Instance a click belongs to is decided by which one's menu
            // carries the id, not by which one the cursor is over: the sprite is
            // free to have walked out from under its own menu.
            let picked: Vec<(InstanceId, menu::MenuAction)> = chosen
                .iter()
                .filter_map(|id| {
                    lives.iter().find_map(|live| {
                        live.menu_hold
                            .as_ref()
                            .and_then(|hold| hold.actions.get(id))
                            .map(|action| (live.id.clone(), action.clone()))
                    })
                })
                .collect();

            for (id, action) in &picked {
                apply_menu_action(
                    action.clone(),
                    &mut roster,
                    &mut lives,
                    &mut slots,
                    id,
                    &rules,
                    &settings,
                    &settings_path,
                    &characters,
                    &mut config,
                    &director,
                    &inspect,
                    &app,
                );
            }
            if !picked.is_empty() {
                remember_instances(&roster, &settings, &settings_path);
            }

            // Tray clicks have no menu_hold: the same ids land here, and the
            // first Instance is the one they apply to when nobody's menu is open.
            if picked.is_empty() {
                for id in &chosen {
                    if let Some(action) = tray_actions.get(id).cloned() {
                        let target = lives
                            .first()
                            .map(|live| live.id.clone())
                            .unwrap_or_default();
                        apply_menu_action(
                            action,
                            &mut roster,
                            &mut lives,
                            &mut slots,
                            &target,
                            &rules,
                            &settings,
                            &settings_path,
                            &characters,
                            &mut config,
                            &director,
                            &inspect,
                            &app,
                        );
                    }
                }
            }

            let mut settings_ops = false;
            while let Ok(op) = ops.try_recv() {
                settings_ops = true;
                match op {
                    SettingsOp::Spawn { character, name } => {
                        spawn_live(
                            &mut roster,
                            &mut lives,
                            &characters,
                            &character,
                            name,
                            &config,
                            &director,
                        );
                    }
                    SettingsOp::Dismiss { id } => {
                        roster.dismiss(&id);
                        lives.retain(|live| live.id != id);
                        slots.abandon(&id);
                    }
                    SettingsOp::SwitchAll { character } => {
                        if let Some(loaded) = characters.get(&character).cloned() {
                            let ids: Vec<_> = lives.iter().map(|live| live.id.clone()).collect();
                            for id in ids {
                                switch_instance(
                                    &mut roster,
                                    &mut lives,
                                    &mut slots,
                                    &id,
                                    Arc::clone(&loaded),
                                    &config,
                                    &director,
                                );
                            }
                        } else {
                            eprintln!("settings: no Character named {character}");
                        }
                    }
                    SettingsOp::Retarget {
                        settings,
                        enabled,
                        ambient_allowed,
                        configured,
                    } => {
                        director = settings;
                        config = model::config_from(&director);
                        config.enabled = enabled;
                        config.ambient_allowed = ambient_allowed;
                        config.configured = configured;
                        if let Ok(mut inspect) = inspect.lock() {
                            inspect.enabled = config.enabled;
                            inspect.configured = config.configured;
                            inspect.ambient_wakes = config.ambient_allowed;
                        }
                        for live in &mut lives {
                            // Completer target changed, not Character. A Wake
                            // still on the wire would propose against the old
                            // host and session; drop it and open a new turn.
                            model::retarget_model(
                                &mut slots,
                                &live.id,
                                &mut live.model,
                                live.character.behaviors.keys().cloned(),
                                &director,
                                configured,
                            );
                        }
                    }
                }
                remember_instances(&roster, &settings, &settings_path);
            }
            publish_instances(&roster, &instance_rows);
            if settings_ops {
                let _ = app.run_on_main_thread(|| {
                    platform::refresh_settings();
                });
            }

            {
                let installed: Vec<String> = characters.keys().cloned().collect();
                let current = lives
                    .first()
                    .map(|live| live.character.name.clone())
                    .unwrap_or_default();
                let id = lives
                    .first()
                    .map(|live| live.id.clone())
                    .unwrap_or_default();
                let settings_now = settings.lock().ok().map(|s| s.clone()).unwrap_or_default();
                let rules_now = rules.lock().ok();
                let description = describe_menu(
                    &installed,
                    &current,
                    &roster,
                    &id,
                    &settings_now,
                    rules_now.as_deref().unwrap_or(&HideRules::default()),
                );
                if let Some(description) = menu::replace_if_changed(&mut last_menu, description) {
                    tray_actions = description.actions.clone();
                    let handle = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        if let Some(state) = handle.try_state::<TrayHandle>() {
                            if let Ok(guard) = state.0.lock() {
                                if let Some(icon) = guard.as_ref() {
                                    if let Err(why) = tray::refresh(icon, &handle, &description) {
                                        eprintln!("tray: {why}");
                                    }
                                }
                            }
                        }
                    });
                }
            }

            if let Ok(settings) = settings.lock() {
                config.ambient_allowed = settings.ambient_wakes;
                config.apply_switch(settings.director_enabled);
                let dnd = settings.do_not_disturb;
                // Reread every tick, like the flags above, so a mute in
                // Settings lands on the next frame and not the next launch.
                sound_allowed = settings.sound_allowed();
                if let Ok(mut inspect) = inspect.lock() {
                    inspect.enabled = config.enabled;
                    inspect.ambient_wakes = settings.ambient_wakes;
                }
                drop(settings);
                for (id, _) in roster.list() {
                    if let Some(instance) = roster.get_mut(&id) {
                        instance.set_do_not_disturb(dnd);
                    }
                }
            }

            if menu_closed {
                for live in lives.iter_mut() {
                    live.menu_hold = None;
                }
            }

            for (index, live) in lives.iter_mut().enumerate() {
                // Only the Instance the press belongs to is told the cursor is
                // over it. The rest are still updated: a pointer that stopped
                // being told the time would measure the next gesture's velocity
                // over the gap.
                live.verbs = live.pointer.update(
                    target == Some(index),
                    held,
                    secondary_held,
                    cursor_points,
                    elapsed_ms,
                );

                if tracing_clicks && !live.verbs.is_empty() {
                    eprintln!("verbs: {} {:?}", live.id, live.verbs);
                }

                // A menu already open holds the Instance still: Verb::Menu is
                // re-injected every tick, which closes the Engine's not-now
                // gates the same way a Poke does. Nothing here waits — the
                // frame loop cannot block, or every other Instance stops too.
                if let Some(hold) = live.menu_hold.as_mut() {
                    hold.elapsed += Duration::from_millis(u64::from(elapsed_ms));
                    if hold.elapsed >= MENU_HOLD_TIMEOUT {
                        eprintln!("menu: hold expired without a close, releasing {}", live.id);
                        live.menu_hold = None;
                    } else if !live.verbs.iter().any(|verb| matches!(verb, Verb::Menu)) {
                        live.verbs.push(Verb::Menu);
                    }
                } else if live.verbs.iter().any(|verb| matches!(verb, Verb::Menu)) {
                    // Same names the cache is keyed by. Folder stems (`trump`)
                    // are not Character names (`Trump`); looking up a stem
                    // would silently refuse the switch.
                    let installed: Vec<String> = characters.keys().cloned().collect();

                    // The Engine's own answer rather than a copy of it, so the
                    // checkbox cannot disagree with what the buddy is doing.
                    let settings_now = settings.lock().ok().map(|s| s.clone()).unwrap_or_default();
                    let rules_now = rules.lock().ok();
                    let description = describe_menu(
                        &installed,
                        &live.character.name,
                        &roster,
                        &live.id,
                        &settings_now,
                        rules_now.as_deref().unwrap_or(&HideRules::default()),
                    );

                    // The overlay the cursor is on, and where in it the cursor
                    // is. A menu is positioned in a window's coordinates, and
                    // there is one window per display.
                    let on_display =
                        display_index_for((cursor_points.x, cursor_points.y), &displays.frames);

                    match on_display.and_then(|index| {
                        displays
                            .frames
                            .get(index)
                            .map(|frame| (overlay_label(index), *frame))
                    }) {
                        Some((label, frame)) => {
                            let at = tauri::LogicalPosition::new(
                                cursor_points.x - frame.x,
                                cursor_points.y - frame.y,
                            );

                            // The description is owned Strings and bools, which
                            // is what lets it cross to the main thread. The menu
                            // itself is built over there: its native objects are
                            // reference-counted without a lock and cannot be
                            // sent, so there is no version of this that builds
                            // the menu here and hands it over.
                            // Kept on this side before the description crosses:
                            // the click comes back as an id, and the id means
                            // what the menu on screen said it meant.
                            let description_actions = description.actions.clone();
                            let handle = app.clone();
                            let signals = menu_sender.clone();
                            let posted = app.run_on_main_thread(move || {
                                if let Err(why) = menu::show(&handle, &description, &label, at) {
                                    eprintln!("menu: {why}");
                                }
                                // Sent whether or not the menu drew, and after
                                // it has closed if the platform's popup is
                                // modal. Either way it is what ends the hold,
                                // because a menu dismissed without a choice
                                // reports nothing anywhere else.
                                let _ = signals.send(MenuSignal::Closed);
                            });

                            match posted {
                                Ok(()) => {
                                    live.menu_hold = Some(MenuHold {
                                        actions: description_actions,
                                        elapsed: Duration::ZERO,
                                    })
                                }
                                // Never held on a menu that was never asked for.
                                Err(why) => {
                                    eprintln!("menu: could not reach the main thread: {why}")
                                }
                            }
                        }
                        None => eprintln!("menu: the cursor is on no known display"),
                    }
                }

                // Grab is on every held tick. Only the first tick of a hold is a
                // pick-up; the rest would otherwise wake the session while
                // dragging.
                let grab_started = live.verbs.iter().any(|verb| matches!(verb, Verb::Grab))
                    && live.last_state != Some(State::Dragged);
                if live.verbs.iter().any(|verb| {
                    matches!(
                        verb,
                        Verb::Poke | Verb::Summon | Verb::Menu | Verb::Throw { .. }
                    )
                }) || grab_started
                {
                    live.addressed = true;
                    live.happened = if live
                        .verbs
                        .iter()
                        .any(|verb| matches!(verb, Verb::Throw { .. }))
                    {
                        Happened::Throw
                    } else if grab_started {
                        Happened::Grab
                    } else if live
                        .verbs
                        .iter()
                        .any(|verb| matches!(verb, Verb::Menu | Verb::Poke))
                    {
                        Happened::Poke
                    } else {
                        Happened::Summon
                    };
                }
            }

            // The Director's clock is the same elapsed time the Engine is
            // given, so a loop that stalled wakes it once on the way back
            // rather than in a burst. Firing takes one interval off the clock
            // rather than zeroing it, for the reason `SnapshotAssembler` gives:
            // zeroing throws the overshoot away and stretches every interval by
            // most of a tick.
            let elapsed = Duration::from_millis(u64::from(elapsed_ms));
            since_sense += elapsed;

            // One reading of the user for every Instance, taken before any of
            // them is ticked so that they all wake against the same desktop. A
            // read per buddy would be the same two calls into AppKit bought N
            // times, and two buddies deciding on idle times a tick apart.
            let sensed = if since_sense >= SENSE_INTERVAL {
                since_sense = since_sense.saturating_sub(SENSE_INTERVAL);
                let mut activity = free_tier.read(&activity_source, &SystemClock);
                if let Ok(settings) = settings.lock() {
                    let denylist = DenyList {
                        excluded_applications: settings.excluded_applications.clone(),
                        filter_password_fields: true,
                    };
                    if activity
                        .frontmost_application
                        .as_deref()
                        .is_some_and(|name| !denylist.allows(name))
                    {
                        activity.frontmost_application = None;
                    }
                }
                last_activity = Some(activity.clone());
                Some(activity)
            } else {
                None
            };
            let displays_asleep = last_activity
                .as_ref()
                .is_some_and(|activity| activity.displays_asleep);

            // Assembled once and handed to every Instance. It carries the
            // desktop, which they share, and the window list is re-read on the
            // assembler's own schedule — asking it once per Instance would poll
            // the window server N times for one answer.
            let mut world = assembler.assemble(elapsed_ms, cursor_points, Vec::new());

            // Whole display frames, not the usable ones physics runs in: the
            // reserved strips are the difference between a fullscreen window
            // and a zoomed one, which is the whole of what is being measured.
            // Rectangles only: whether a window has taken a whole screen is a
            // question about geometry, and `visibility` has no use for which
            // window it is.
            let rects: Vec<_> = world.windows.iter().map(|window| window.rect).collect();
            let desktop = Desktop {
                fullscreen_frontmost: fullscreen_frontmost(&rects, &displays.frames),
            };

            // Where each Instance ends up, in the space every display shares.
            let mut placed: Vec<Placed> = Vec::with_capacity(lives.len());

            // The window list is re-read at the frame rate while any Instance is
            // riding a moving window. One riding buddy is reason enough: the
            // others cost nothing extra, the read being shared.
            let mut riding = false;

            // Whether the cursor is over any Instance's art. Click-through is a
            // property of the overlay, which every Instance shares, so one
            // sprite under the cursor is enough to make the overlay take the
            // click — and the press is then routed to that one Instance.
            let mut over_sprite = false;

            for live in lives.iter_mut() {
                let Some(instance) = roster.get_mut(&live.id) else {
                    continue; // dismissed; the retain above has already dropped it
                };

                live.since_wake += elapsed;
                live.since_state += elapsed;
                if !displays_asleep {
                    live.since_ambient += elapsed;
                }

                let mut proposal = None;
                let arrived = slots.take(&live.id);
                let applied = arrived.is_some();
                if let Some((wake, context)) = arrived {
                    if model::tracing() {
                        match &wake {
                            Wake::Proposed(parsed) if !parsed.behavior.is_empty() => eprintln!(
                                "director: {} parsed {}{}",
                                live.id,
                                parsed.behavior,
                                parsed
                                    .dialogue
                                    .as_deref()
                                    .map(|line| format!(" | {line}"))
                                    .unwrap_or_default(),
                            ),
                            Wake::Proposed(_) => {}
                            Wake::Failed => {
                                eprintln!("director: {} failed; Static fallback", live.id)
                            }
                        }
                    }
                    proposal = director::fallback(wake, &mut live.director, &context);
                    if model::tracing() {
                        match &proposal {
                            Some(playing) if playing.behavior.is_empty() => {
                                eprintln!(
                                    "director: {} saying {}",
                                    live.id,
                                    playing.dialogue.as_deref().unwrap_or("")
                                );
                            }
                            Some(playing) => eprintln!(
                                "director: {} playing {}{}",
                                live.id,
                                playing.behavior,
                                playing
                                    .dialogue
                                    .as_deref()
                                    .map(|line| format!(" | {line}"))
                                    .unwrap_or_default(),
                            ),
                            None => eprintln!("director: {} nothing to play", live.id),
                        }
                    }
                }

                if let Some(activity) = &sensed {
                    let due = director::due(
                        live.since_wake,
                        config.wake_every,
                        activity,
                        live.previous_idle,
                        live.since_state,
                        instance.do_not_disturb(),
                    );
                    live.previous_idle = activity.idle;

                    if due {
                        live.since_wake = live.since_wake.saturating_sub(config.wake_every);
                        live.since_state = Duration::ZERO;
                        // Static keeps the free life going. A session call in
                        // flight is the one exception: do not stack a weight pick
                        // on a proposal that is about to land.
                        if !slots.waiting(&live.id) && !applied {
                            proposal = live.director.propose(&Context {
                                activity: activity.clone(),
                                recent: live.recent.clone(),
                                personality: live.character.personality.clone(),
                                state: live.last_state.unwrap_or(State::Grounded),
                                happened: live.happened,
                                standing: String::new(),
                            });
                        }
                    }
                }

                // The verbs are this Instance's alone, decided above. Taken
                // rather than cloned: the snapshot is reused across Instances,
                // and a verb left behind would be replayed next tick.
                world.verbs = std::mem::take(&mut live.verbs);
                world.proposal = proposal;

                let frame = instance.tick(&world);
                riding |= frame.riding;

                // Engine names a Poke and a Dwell. The pointer loop also
                // marks verbs so the wake can say `happened: poked`; this
                // is the bit that must not be dropped or a click never
                // reaches the session.
                if frame.addressed {
                    live.addressed = true;
                }

                let became_perched = live.last_state.is_some()
                    && frame.state == State::Perched
                    && live.last_state != Some(State::Perched);
                if became_perched {
                    live.addressed = true;
                    live.happened = Happened::Perch;
                }

                if live.last_state != Some(frame.state) {
                    live.last_state = Some(frame.state);
                    live.since_state = Duration::ZERO;
                }

                // After the tick so a Throw is already Falling, not still Dragged.
                let happened = live.happened;
                let reactive_wake =
                    if let (Some(model), Some(activity)) = (&live.model, last_activity.as_ref()) {
                        if director::session_due(
                            live.addressed,
                            live.since_ambient,
                            &live.pace,
                            activity.displays_asleep,
                            instance.do_not_disturb(),
                            config.ambient_allowed,
                        ) && config.enabled
                        {
                            let context = Context {
                                activity: activity.clone(),
                                recent: live.recent.clone(),
                                personality: live.character.personality.clone(),
                                state: frame.state,
                                happened: live.happened,
                                standing: assembler.standing_on(frame.position),
                            };
                            let was_addressed = live.addressed;
                            if live.addressed {
                                live.pace.after_reactive();
                            } else {
                                live.pace.after_ambient();
                            }
                            live.addressed = false;
                            live.happened = Happened::Ambient;
                            live.since_ambient = Duration::ZERO;
                            let payload = model.prompt(&context);
                            // One panel for however many Instances are running, so
                            // the newest call is what it shows. #18 owns the panel
                            // and can give it a buddy to choose; until then, the
                            // last payload sent is the honest answer to "what was
                            // sent", whichever Instance sent it.
                            if let Ok(mut inspect) = inspect.lock() {
                                inspect.last_payload = Some(payload);
                                inspect.wake_secs = live.pace.wait().as_secs();
                            }
                            slots.wake(&live.id, Arc::clone(model), context);
                            was_addressed
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                if tracing_clicks && (frame.addressed || reactive_wake || !world.verbs.is_empty()) {
                    let skip = if reactive_wake {
                        "started"
                    } else if live.model.is_none() {
                        "no-model"
                    } else if last_activity.is_none() {
                        "no-activity-yet"
                    } else if instance.do_not_disturb() {
                        "dnd"
                    } else if last_activity
                        .as_ref()
                        .is_some_and(|activity| activity.displays_asleep)
                    {
                        "asleep"
                    } else {
                        "not-due"
                    };
                    eprintln!(
                        "session: {} addressed={} happened={happened:?} {skip}",
                        live.id, live.addressed,
                    );
                }

                let thinking =
                    (reactive_wake || slots.thinking(&live.id)) && !instance.do_not_disturb();

                // What the user has seen is what the Engine played, not what the
                // Director asked for: a proposal the State refuses never reaches
                // the screen, and suppressing it would silence a Behavior nobody
                // watched.
                if let Some(played) = &frame.behavior {
                    director::remember(&mut live.recent, played.clone());
                    if tracing_frames {
                        eprintln!("director: {} {played}", live.id);
                    }
                }

                // On change, not per tick: the loop turns at display rate and
                // an unconditional line would bury every other trace in the
                // log. Above the `draw` below rather than beside the frame
                // line, so a Character whose art will not draw — the one case
                // that skips the rest of this Instance — still says what its
                // Engine was doing.
                //
                // A dash, not a blank, for the Engine's own moments: a Land or
                // a startle has no Behavior name because no Director proposed
                // it, and an empty pair of brackets reads as a bug in the
                // trace rather than as the answer.
                if tracing_engine {
                    let now = Traced {
                        behavior: frame.playing_behavior.clone(),
                        primitive: frame.playing_primitive,
                        animation: frame.animation,
                        state: frame.state,
                    };
                    if live.traced_last.as_ref() != Some(&now) {
                        let at_ms = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map_or(0, |since| since.as_millis());
                        let primitive = match now.primitive {
                            Some(primitive) => format!("{primitive:?}"),
                            None => "-".to_string(),
                        };
                        eprintln!(
                            "engine: {at_ms} behavior({}) primitive({primitive}) animation({}) state({:?}) {}",
                            now.behavior.as_deref().unwrap_or("-"),
                            now.animation,
                            now.state,
                            live.id,
                        );
                        live.traced_last = Some(now);
                    }
                } else {
                    // Forgotten while off, so flipping the switch back on opens
                    // with a line instead of waiting for the next change.
                    live.traced_last = None;
                }

                // The Engine names an Animation and how long it has been
                // playing; the Character Manifest says what that means in
                // frames. Resolving it here rather than in the webview keeps the
                // frame the hit-test measures and the frame the user sees the
                // same one.
                // A Character with no drawable Animation at all, which a
                // validated Character Package cannot be. Left out of `placed`,
                // and the webview reads absence as dismissal: it takes the
                // sprite away, bubble and interpolation with it. That is the
                // right answer for art that cannot be drawn, and the reason
                // nothing else in this loop may skip an Instance silently.
                let Some(drawn) =
                    live.character
                        .draw(frame.animation, frame.animation_ms, frame.variant_draw)
                else {
                    continue;
                };
                let scale = live.character.scale as i32;
                let (width, height) = (
                    drawn.frame_size.0 as i32 * scale,
                    drawn.frame_size.1 as i32 * scale,
                );

                // Placed once, in the space every display shares. Each overlay
                // is handed it in its own coordinates below.
                let sprite =
                    place_sprite((frame.position.x, frame.position.y), (width, height), scale);

                if tracing_frames {
                    // Unix milliseconds, so that a prop window opened by the
                    // verification script and this loop can be read against one
                    // clock. Only read when tracing: the loop needs elapsed
                    // time, never the time of day.
                    let at_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |since| since.as_millis());

                    // The Instance last, so that everything before it stands
                    // where it did when one buddy was all there was.
                    // scripts/verify-overlay.sh matches on this prefix, runs a
                    // single Instance, and has no use for which one.
                    eprintln!(
                        "frame: {} {:?} pos({:.0},{:.0}) sprite({},{}) {}#{} {}",
                        at_ms,
                        frame.state,
                        frame.position.x,
                        frame.position.y,
                        sprite.x,
                        sprite.y,
                        frame.animation,
                        drawn.index,
                        live.id,
                    );
                }

                // The tick's second hit-test, against the sprite about to be
                // drawn rather than the one last drawn: whether the next click
                // should reach us is a question about where the art is going to
                // be. A cursor that has just arrived over it must not spend a
                // frame passing clicks to the application underneath.
                let mirrored = frame.facing < 0.0;
                over_sprite |= drawn.mask.hit(&sprite, cursor_at.0, cursor_at.1, mirrored);
                live.drawn_last = Some(Drawn {
                    rect: sprite,
                    animation: frame.animation,
                    animation_ms: frame.animation_ms,
                    variant_draw: frame.variant_draw,
                    mirrored,
                });

                let owner = bubble_owner((frame.position.x, frame.position.y), &displays.frames);
                let dialogue = super::carry_line(
                    &mut live.spoken,
                    frame.dialogue.as_deref(),
                    owner,
                    Instant::now(),
                );

                placed.push(Placed {
                    id: live.id.clone(),
                    character: live.character.name.clone(),
                    sprite,
                    width,
                    height,
                    animation: drawn.animation.to_string(),
                    frame_index: drawn.index,
                    facing: frame.facing as i8,
                    dialogue,
                    thinking,
                    cue: frame.cue,
                    owner,
                    mask: drawn.mask.clone(),
                });
            }

            assembler.poll_fast(riding);

            // The log is what is silent on almost every tick, not the
            // renderer: only a change is worth a line, and a fullscreen
            // application held for an hour is one of them rather than one an
            // Engine tick.
            let presence = rules
                .lock()
                .map(|mut rules| {
                    if let Some(change) = rules.update(desktop) {
                        // Unconditional, unlike the traces above, because it is
                        // rare — a handful of lines in a session — and because
                        // whether a rule fired is the first thing anyone
                        // checking hiding by hand needs to know.
                        eprintln!(
                            "presence: {} over {}ms",
                            if change.visible { "shown" } else { "hidden" },
                            change.fade_ms,
                        );
                    }
                    rules.presence()
                })
                .unwrap_or(Change {
                    visible,
                    fade_ms: 0,
                });

            // A display can be plugged in, unplugged or rearranged while the
            // app runs, and every display needs its overlay. Posted rather than
            // done here: only the main thread may build a window.
            //
            // Recorded by the closure, on success, rather than here once the
            // post is accepted. An accepted post only means the work is queued,
            // and every failure inside `place_overlays` is a log line rather
            // than a refusal; recording it here would latch a desktop the
            // overlays never reached and leave a hot-plugged display with no
            // overlay until the display list changed again. The price is that a
            // reconcile that keeps failing is posted again every tick, which is
            // what retrying it means.
            //
            // An empty read is ignored rather than obeyed. It is what a failed
            // read of the desktop looks like as well as a machine with no
            // screen, and tearing every overlay down costs two webviews and
            // their art to rebuild — for a desktop that has nothing to draw on
            // either way.
            if !displays.frames.is_empty() && *covered.lock().unwrap() != displays.frames {
                let handle = app.clone();
                let frames = displays.frames.clone();
                let placed = Arc::clone(&covered);
                let _ = app.run_on_main_thread(move || match place_overlays(&handle, &frames) {
                    Ok(()) => *placed.lock().unwrap() = frames,
                    Err(why) => eprintln!("overlay: {why}"),
                });
            }

            // Click-through returns wherever no sprite is drawn, and everywhere
            // while the Character is hidden — a Character nobody can see must
            // not swallow a click. The exception is a held Character: a drag
            // that outruns the art would otherwise put the cursor over
            // transparent pixels, hand the button to whatever is underneath,
            // and drop the sprite in the user's hand.
            let holding = lives.iter().any(|live| live.pointer.grabbing());
            let ignore = !(presence.visible && (over_sprite || holding));
            let mut flipped = false;

            for (index, display) in displays.frames.iter().enumerate() {
                let label = overlay_label(index);
                let Some(window) = app.get_webview_window(&label) else {
                    continue; // a display whose overlay has not been built yet
                };

                // Retried until it succeeds: GTK has no window handle until the
                // widget is realized (shown and ticked), and `window_handle`
                // must run on the GTK main thread.
                #[cfg(all(unix, not(target_os = "macos")))]
                if !configured
                    .lock()
                    .unwrap()
                    .get(index)
                    .copied()
                    .unwrap_or(false)
                    && !configure_in_flight
                        .lock()
                        .unwrap()
                        .get(index)
                        .copied()
                        .unwrap_or(false)
                {
                    configure_in_flight.lock().unwrap()[index] = true;
                    let handle = app.clone();
                    let label_clone = label.clone();
                    let configured_clone = Arc::clone(&configured);
                    let configure_in_flight_clone = Arc::clone(&configure_in_flight);
                    let overlay_index = index;
                    let trace = tracing;
                    let _ = app.run_on_main_thread(move || {
                        configure_in_flight_clone.lock().unwrap()[overlay_index] = false;
                        if let Some(window) = handle.get_webview_window(&label_clone) {
                            match platform::configure_overlay(&window) {
                                Ok(()) => {
                                    configured_clone.lock().unwrap()[overlay_index] = true;
                                    if trace {
                                        eprintln!("overlay: {label_clone} EWMH configured");
                                    }
                                }
                                Err(e) => {
                                    static LOGGED: std::sync::atomic::AtomicBool =
                                        std::sync::atomic::AtomicBool::new(false);
                                    if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                                        eprintln!(
                                            "overlay: {label_clone} EWMH config deferred: {e}"
                                        );
                                    }
                                }
                            }
                        }
                    });
                }

                // Every overlay is told about every Instance, including the ones
                // no sprite is anywhere near: each draws the part that falls
                // inside it, which is what leaves a Character on a seam whole
                // instead of clipped to one display.
                //
                // Addressed rather than emitted to all, because each overlay
                // is told a different set of rectangles. src/main.js has to name
                // its own label to match: an untargeted listener hears every
                // emit, addressed elsewhere or not, and would draw whichever
                // display's rectangles arrived last.
                let sprites = placed
                    .iter()
                    .map(|instance| {
                        let local = instance.sprite.in_overlay(*display);
                        SpritePlacement {
                            id: &instance.id,
                            character: &instance.character,
                            x: local.x,
                            y: local.y,
                            width: instance.width,
                            height: instance.height,
                            animation: &instance.animation,
                            frame_index: instance.frame_index,
                            facing: instance.facing,
                            dialogue: instance.dialogue.clone(),
                            thinking: instance.thinking,
                            // Every overlay draws the art; one draws the
                            // bubble (#178, `bubble_owner`), and `forOverlay`
                            // strips the rest from the ones that lost.
                            bubble: instance.owner == Some(index),
                            cue: instance.cue.map(Cue::name),
                        }
                    })
                    .collect();

                let _ = window.emit_to(
                    &label,
                    FRAME_EVENT,
                    Placement {
                        sprites,
                        visible: presence.visible,
                        fade_ms: presence.fade_ms,
                        sound: sound_allowed,
                    },
                );

                // Click-through is per-window, and a click only ever lands on
                // the overlay the cursor is on. Every other overlay passes
                // clicks through whatever the sprite is doing, so a click on
                // one display is never swallowed by a sprite on another.
                let ignore = ignore || on_overlay != Some(index);

                // On X11, use per-pixel click-through via XShapeCombineMask.
                // XShape carves the input region, but Tauri must also receive events.
                // window_handle() requires the GTK main thread, so marshal the X11
                // calls. Only set ignore-cursor-events false after the mask applies,
                // or the overlay becomes a fullscreen click-eater.
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    if !ignore && presence.visible {
                        let sprite_on_overlay = placed.iter().find(|instance| {
                            let local = instance.sprite.in_overlay(*display);
                            // Sprite is on this overlay if any part of it is visible
                            local.x + instance.width > 0
                                && local.x < display.width as i32
                                && local.y + instance.height > 0
                                && local.y < display.height as i32
                        });

                        if let Some(instance) = sprite_on_overlay {
                            let local = instance.sprite.in_overlay(*display);
                            let (_width, _height, opaque) = instance.mask.raw();
                            let mask_params = (
                                Some(opaque.to_vec()),
                                local.x,
                                local.y,
                                i32::from(instance.facing),
                                instance.sprite.scale,
                            );

                            // `last_mask` exists so an unchanged sprite does not
                            // rebuild the pixmap every 16ms.
                            if last_mask.lock().unwrap().get(index) != Some(&mask_params)
                                && !mask_in_flight
                                    .lock()
                                    .unwrap()
                                    .get(index)
                                    .copied()
                                    .unwrap_or(false)
                            {
                                mask_in_flight.lock().unwrap()[index] = true;
                                let handle = app.clone();
                                let label_clone = label.clone();
                                let mask_clone = instance.mask.clone();
                                let sprite_x = local.x;
                                let sprite_y = local.y;
                                let sprite_facing = i32::from(instance.facing);
                                let sprite_scale = instance.sprite.scale;
                                let mask_applied_clone = Arc::clone(&mask_applied);
                                let last_mask_clone = Arc::clone(&last_mask);
                                let mask_in_flight_clone = Arc::clone(&mask_in_flight);
                                let mask_params_clone = mask_params.clone();
                                let overlay_index = index;
                                let trace = tracing;

                                let _ = app.run_on_main_thread(move || {
                                    mask_in_flight_clone.lock().unwrap()[overlay_index] = false;
                                    if let Some(window) = handle.get_webview_window(&label_clone) {
                                        match platform::update_input_region(
                                            &window,
                                            Some(&mask_clone),
                                            sprite_x,
                                            sprite_y,
                                            sprite_facing,
                                            sprite_scale,
                                        ) {
                                            Ok(()) => {
                                                mask_applied_clone.lock().unwrap()[overlay_index] =
                                                    true;
                                                last_mask_clone.lock().unwrap()[overlay_index] =
                                                    mask_params_clone;
                                                if trace {
                                                    eprintln!(
                                                        "overlay: {label_clone} input mask applied"
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                static LOGGED: std::sync::atomic::AtomicBool =
                                                    std::sync::atomic::AtomicBool::new(false);
                                                if !LOGGED
                                                    .swap(true, std::sync::atomic::Ordering::Relaxed)
                                                {
                                                    eprintln!("overlay: {label_clone} update_input_region deferred: {e}");
                                                }
                                            }
                                        }
                                    }
                                });
                            }

                            if mask_applied
                                .lock()
                                .unwrap()
                                .get(index)
                                .copied()
                                .unwrap_or(false)
                                && ignoring[index] != Some(false)
                            {
                                flipped = true;
                                if window.set_ignore_cursor_events(false).is_ok() {
                                    ignoring[index] = Some(false);
                                }
                            }
                        } else {
                            // No sprite on this overlay, make it fully click-through
                            let mask_params = (None, 0, 0, 1, 1);

                            if last_mask.lock().unwrap().get(index) != Some(&mask_params) {
                                let handle = app.clone();
                                let label_clone = label.clone();
                                let last_mask_clone = Arc::clone(&last_mask);
                                let mask_params_clone = mask_params.clone();
                                let overlay_index = index;

                                let _ = app.run_on_main_thread(move || {
                                    if let Some(window) = handle.get_webview_window(&label_clone) {
                                        if platform::update_input_region(&window, None, 0, 0, 1, 1)
                                            .is_ok()
                                        {
                                            last_mask_clone.lock().unwrap()[overlay_index] =
                                                mask_params_clone;
                                        }
                                    }
                                });
                            }

                            if ignoring[index] != Some(true) {
                                flipped = true;
                                if window.set_ignore_cursor_events(true).is_ok() {
                                    ignoring[index] = Some(true);
                                }
                            }
                        }
                    } else {
                        // Ignoring or invisible: make the whole window click-through
                        let mask_params = (None, 0, 0, 1, 1);

                        if last_mask.lock().unwrap().get(index) != Some(&mask_params) {
                            let handle = app.clone();
                            let label_clone = label.clone();
                            let last_mask_clone = Arc::clone(&last_mask);
                            let mask_params_clone = mask_params.clone();
                            let overlay_index = index;

                            let _ = app.run_on_main_thread(move || {
                                if let Some(window) = handle.get_webview_window(&label_clone) {
                                    if platform::update_input_region(&window, None, 0, 0, 1, 1)
                                        .is_ok()
                                    {
                                        last_mask_clone.lock().unwrap()[overlay_index] =
                                            mask_params_clone;
                                    }
                                }
                            });
                        }

                        if ignoring[index] != Some(true) {
                            flipped = true;
                            if window.set_ignore_cursor_events(true).is_ok() {
                                ignoring[index] = Some(true);
                            }
                        }
                    }
                }

                // On macOS and other platforms, use Tauri's boolean click-through only
                #[cfg(not(all(unix, not(target_os = "macos"))))]
                {
                    // Only record the new state once the platform accepted it.
                    // Recording it regardless would latch a failed toggle forever,
                    // leaving click-through stuck in whichever mode it happened to
                    // be in.
                    if ignoring[index] != Some(ignore) {
                        flipped = true;
                        if window.set_ignore_cursor_events(ignore).is_ok() {
                            ignoring[index] = Some(ignore);
                        }
                    }
                }
            }

            ticks = ticks.wrapping_add(1);
            if tracing && (flipped || ticks % 120 == 0) {
                eprintln!(
                    "hit-test: cursor({:.0},{:.0}) scale {:.1} -> point({},{}) \
                     on overlay {} {} click-through {}{}",
                    cursor.x,
                    cursor.y,
                    cursor_scale,
                    cursor_at.0,
                    cursor_at.1,
                    on_overlay.map_or(-1, |index| index as i32),
                    if over_sprite { "HIT " } else { "miss" },
                    if ignore { "on" } else { "OFF" },
                    if flipped { "  <- flipped" } else { "" },
                );
            }
        }
    });
}
