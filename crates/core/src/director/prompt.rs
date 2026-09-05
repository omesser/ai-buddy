use super::{Context, Happened, State, CHAT_LIMIT};

/// The opening turn: who this is, what it may propose, and this moment.
///
/// Later wakes send `follow_up` only. The Completer holds the conversation
/// so the Personality Prompt is not paid for again.
pub fn character_prompt(
    context: &Context,
    behaviors: impl IntoIterator<Item = impl AsRef<str>>,
) -> String {
    let names: Vec<String> = behaviors
        .into_iter()
        .map(|name| name.as_ref().to_string())
        .collect();
    let declared = if names.is_empty() {
        "(none)".to_string()
    } else {
        names.join(", ")
    };
    let personality = if context.personality.is_empty() {
        "(no personality)"
    } else {
        context.personality.as_str()
    };

    // The universal voice rules, written once for every Character rather
    // than copied into personality files to drift (#156). A personality
    // supplies the material; this paragraph governs the delivery.
    format!(
        "{personality}\n\
         \n\
         You may propose one of these behaviors: {declared}\n\
         \n\
         Reply with the behavior name on the first line.\n\
         An optional spoken line may follow on the next line.\n\
         Propose nothing else.\n\
         \n\
         Speak in this character's voice, always in character: never mention \
         being a model or an assistant. A spoken line fits a small speech \
         bubble: five short sentences at the most. Vary: prefer a line you \
         have not used yet, though a signature phrase may recur, and \
         lean away from the behaviors listed as recently played. React to \
         this moment when there is something worth remarking on: what just \
         happened to you, and what you are standing on. Dialogue is \
         demeanour, never capability: never promise an action on the machine \
         or claim an ability.\n\
         \n\
         {}",
        follow_up(context)
    )
}

/// The one word the prompt uses for each `Happened`.
pub fn happened_word(happened: &Happened) -> &'static str {
    match happened {
        Happened::Poke => "poked",
        Happened::Throw => "thrown",
        Happened::Summon => "summoned",
        Happened::Grab => "picked up",
        Happened::Perch => "placed on a perch",
        Happened::Chat(_) => "spoken to",
        Happened::Ambient => "time passed",
    }
}

/// A later turn in the same session. No Personality Prompt, no roster.
pub fn follow_up(context: &Context) -> String {
    let recent = if context.recent.is_empty() {
        "(none)".to_string()
    } else {
        context.recent.join(", ")
    };
    let clock = format_clock(context.activity.hour, context.activity.minute);
    let happened = happened_word(&context.happened);
    let state = match context.state {
        State::Grounded => "idle",
        State::Falling => "falling",
        State::Dragged => "held",
        State::Perched => "perched",
        State::Climbing => "climbing",
        State::Asleep => "asleep",
    };
    let open = match context.activity.frontmost_application.as_deref() {
        Some(name) if !name.is_empty() => format!("{name} is the frontmost window"),
        _ => "nothing is frontmost".to_string(),
    };

    // Last, after every labelled fact, because it is the only line the user
    // writes: a paste imitating `state:` or `open:` then reads as part of what
    // was said and cannot displace the real value above it.
    let said = match &context.happened {
        Happened::Chat(line) => format!("they said: {}\n", cut(line)),
        _ => String::new(),
    };

    format!(
        "what just happened: {happened}\n\
         recent: {recent}\n\
         time: {clock}\n\
         state: {state}\n\
         standing on: {standing}\n\
         open: {open}\n\
         {said}",
        standing = if context.standing.is_empty() {
            "nothing"
        } else {
            context.standing.as_str()
        },
    )
}

/// `line` at `CHAT_LIMIT` characters, cut on a character boundary so a
/// multi-byte paste cannot panic the slice.
fn cut(line: &str) -> &str {
    match line.char_indices().nth(CHAT_LIMIT) {
        Some((end, _)) => &line[..end],
        None => line,
    }
}

fn format_clock(hour: u8, minute: u8) -> String {
    format!("{hour:02}:{minute:02}")
}
