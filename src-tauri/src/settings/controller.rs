//! What a gesture on the settings form means, with no window in it.
//!
//! `form.rs` says how the form is drawn; this says what happens when it is
//! used. Every renderer carried its own copy of that second half, and the copy
//! is where #534 went wrong, so the decisions live here and the renderer is
//! left holding only the widgets: it reports a row id and a value, and does
//! what the `Outcome` says.

use crate::settings::form::{FormDescription, RowOperation};
use crate::settings::{DirectorDraft, SettingsPatch, SettingsView};

/// What the surface reports a user did, by row id.
///
/// A `Pick` is a `SetText` that came off a list, and it is separate only
/// because a list re-reports the value already shown (#452).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    SetBool {
        id: String,
        value: bool,
    },
    SetText {
        id: String,
        value: String,
    },
    Pick {
        id: String,
        value: String,
    },
    /// A shortcut list, which writes the row below it rather than one of its
    /// own. `current` is what that row holds now, which only the surface can
    /// read (#670).
    Shortcut {
        id: String,
        value: String,
        current: String,
    },
    Press {
        id: String,
    },
}

/// What the surface must do about it.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The gesture named no write: an unknown row, a frozen one, or a value
    /// that is already what is shown.
    Nothing,
    /// Send it through `SettingsSession::apply`.
    Apply(SettingsPatch),
    /// Apply, then redraw. Only a pick that raises no `SettingsOp` needs the
    /// redraw, and asking for it always is cheaper than telling which (#577).
    ApplyAndRefresh(SettingsPatch),
    /// Apply's two halves: write the patch if there is one, and only then take
    /// the Director tab back to live state. A locked Keychain fails the write,
    /// and resetting anyway would discard an edit nothing saved (#279).
    Commit(Option<SettingsPatch>),
    /// Back to live state, writing nothing.
    Reset,
    /// Stage the key delete: blank the key field and arm Apply, without
    /// touching the store (#279).
    ClearKey,
    /// Put `value` in row `id`, then apply `patch` — or, when there is none,
    /// let the row sit staged until Apply.
    Fill {
        id: &'static str,
        value: &'static str,
        patch: Option<SettingsPatch>,
    },
    /// A modal, a file, the clipboard, a spawn: the operations that need more
    /// than a patch, and that only the surface can perform.
    Run(RowOperation),
}

/// `draft` is the Director tab as the surface holds it, and carries the
/// `FormDescription` every decision here is taken against — the caller's, so
/// the row lookup and the frozen rule answer from one description.
pub fn handle(event: &Event, draft: &DirectorDraft<'_>, view: &SettingsView) -> Outcome {
    let description = draft.description;
    match event {
        Event::SetBool { id, value } => match description.bool_write(id) {
            Some(field) => {
                let mut patch = SettingsPatch::default();
                patch.set_bool(field, *value);
                Outcome::Apply(patch)
            }
            None => Outcome::Nothing,
        },
        Event::SetText { id, value } => {
            text_patch(description, id, value).map_or(Outcome::Nothing, Outcome::Apply)
        }
        Event::Pick { id, value } => {
            // A list sends its action for a click on the item already
            // selected. Writing that back is a save and a redraw for nothing,
            // and on the source list it is lossy (#452).
            if view.popup_value(id).as_deref() == Some(value.as_str()) {
                return Outcome::Nothing;
            }
            text_patch(description, id, value).map_or(Outcome::Nothing, Outcome::ApplyAndRefresh)
        }
        Event::Shortcut { id, value, current } => shortcut(description, id, value, current),
        Event::Press { id } => match description.operations.get(id) {
            Some(RowOperation::Apply) => Outcome::Commit(draft.patch(view)),
            Some(RowOperation::Cancel) => Outcome::Reset,
            Some(RowOperation::ClearKey) => Outcome::ClearKey,
            // Carries no row, so a staged edit beside it is neither written
            // nor thrown away: this button is a session boundary and nothing
            // else (#679).
            Some(RowOperation::NewSession) => Outcome::Apply(SettingsPatch {
                new_session: true,
                ..SettingsPatch::default()
            }),
            Some(op) => Outcome::Run(op.clone()),
            None => Outcome::Nothing,
        },
    }
}

fn shortcut(description: &FormDescription, id: &str, value: &str, current: &str) -> Outcome {
    let Some(shortcut) = description.shortcut(id) else {
        return Outcome::Nothing;
    };
    // Custom, and any title off the list, name nothing to write: the row below
    // is what a value this list cannot spell is.
    let Some(value) = (shortcut.value)(value) else {
        return Outcome::Nothing;
    };
    if current == value {
        return Outcome::Nothing;
    }
    Outcome::Fill {
        id: shortcut.row,
        value,
        // A batched row stages: the four Director rows only apply together, so
        // Apply is what reaches the file (#279). An unbatched one has no Apply
        // beside it and saves here (#638).
        patch: if description.text_batched(shortcut.row) {
            None
        } else {
            text_patch(description, shortcut.row, value)
        },
    }
}

/// `None` when the row writes nothing, is frozen, or the value is one
/// `set_text` refuses — all of which are the same answer to the surface.
fn text_patch(description: &FormDescription, id: &str, value: &str) -> Option<SettingsPatch> {
    if description.frozen(id) {
        return None;
    }
    let field = description.text_write(id)?;
    let mut patch = SettingsPatch::default();
    patch.set_text(field, value).then_some(patch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model;
    use crate::settings::form;
    use crate::settings::Settings;
    use std::path::Path;

    const BASE_URL: &str = "https://api.openai.com";
    const MODEL: &str = "gpt-4o-mini";

    /// The Director tab's live state: a saved Base URL and Model, no key.
    fn director_view() -> SettingsView {
        SettingsView::from_parts(
            &Settings {
                director_base_url: BASE_URL.into(),
                director_model: MODEL.into(),
                ..Settings::default()
            },
            Path::new("/tmp/ai-buddy/memory.md"),
            None,
            Vec::new(),
            Vec::new(),
            (false, String::new(), String::new()),
            None,
        )
    }

    /// A window that has drawn itself from `view` and has not been typed into.
    fn drawn<'a>(view: &SettingsView, description: &'a FormDescription) -> DirectorDraft<'a> {
        DirectorDraft {
            base_url: view.director_base_url.clone(),
            model: view.director_model.clone(),
            key: String::new(),
            clear_key: false,
            description,
        }
    }

    fn press(id: &str) -> Event {
        Event::Press { id: id.into() }
    }

    /// #534: Apply on a Director tab nobody touched wiped the saved Base URL
    /// and Model with empty strings. The window read its own blank fields back
    /// as an edit. Nothing to apply is the answer this end has to give, or a
    /// second surface can make the same mistake.
    #[test]
    fn an_untouched_director_tab_applies_nothing() {
        model::tests::with_env(None, None, None, || {
            let view = director_view();
            let description = form::describe();
            assert_eq!(
                handle(&press(form::APPLY_ID), &drawn(&view, &description), &view),
                Outcome::Commit(None),
            );
        });
    }

    /// The other half of #534, and the reason the fix was not a blank guard
    /// inside `DirectorDraft::edit`: clearing a field really is an edit the
    /// user can express, so the controller cannot tell an untouched tab from a
    /// cleared one. Only a window that draws before it reads itself back can,
    /// which is where #534 was fixed and where it has to stay fixed.
    #[test]
    fn a_blank_field_is_an_edit_not_an_untouched_row() {
        model::tests::with_env(None, None, None, || {
            let view = director_view();
            let description = form::describe();
            let blank = DirectorDraft {
                base_url: String::new(),
                model: String::new(),
                ..drawn(&view, &description)
            };
            let Outcome::Commit(Some(patch)) = handle(&press(form::APPLY_ID), &blank, &view) else {
                panic!("a cleared pair of rows is a patch");
            };
            assert_eq!(patch.director_base_url.as_deref(), Some(""));
            assert_eq!(patch.director_model.as_deref(), Some(""));
        });
    }

    /// One typed row does not drag the other into the patch, which is what
    /// keeps an Apply from writing back stale text beside the edit (#279).
    #[test]
    fn apply_carries_only_the_row_that_changed() {
        model::tests::with_env(None, None, None, || {
            let view = director_view();
            let description = form::describe();
            let typed = DirectorDraft {
                base_url: "https://api.x.ai".into(),
                ..drawn(&view, &description)
            };
            let Outcome::Commit(Some(patch)) = handle(&press(form::APPLY_ID), &typed, &view) else {
                panic!("a typed URL is a patch");
            };
            assert_eq!(patch.director_base_url.as_deref(), Some("https://api.x.ai"));
            assert!(patch.director_model.is_none());
        });
    }

    /// Cancel writes nothing at all — the reset is the whole of it.
    #[test]
    fn cancel_writes_nothing() {
        model::tests::with_env(None, None, None, || {
            let view = director_view();
            let description = form::describe();
            let typed = DirectorDraft {
                model: "grok-4.6".into(),
                ..drawn(&view, &description)
            };
            assert_eq!(
                handle(&press(form::CANCEL_ID), &typed, &view),
                Outcome::Reset,
            );
        });
    }

    /// #272: an exported variable owns the row, and `model::resolve` would
    /// give it the last word anyway — so the edit is refused here rather than
    /// written and then ignored.
    #[test]
    fn a_frozen_row_refuses_a_direct_write() {
        model::tests::with_env(None, Some("https://env.example"), None, || {
            let view = director_view();
            let description = form::describe();
            assert!(
                description.frozen(form::DIRECTOR_BASE_URL_ID),
                "precondition: the variable owns the URL row"
            );
            let event = Event::SetText {
                id: form::DIRECTOR_BASE_URL_ID.into(),
                value: "https://typed.example".into(),
            };
            assert_eq!(
                handle(&event, &drawn(&view, &description), &view),
                Outcome::Nothing,
            );
        });
    }

    /// #452: re-picking what the list already shows is a save and a redraw for
    /// nothing, and on the source list it loses the current value.
    #[test]
    fn picking_the_value_already_shown_writes_nothing() {
        model::tests::with_env(None, None, None, || {
            let view = director_view();
            let description = form::describe();
            let shown = view
                .popup_value(form::CHARACTER_ID)
                .expect("the Character row is a list");
            let event = Event::Pick {
                id: form::CHARACTER_ID.into(),
                value: shown,
            };
            assert_eq!(
                handle(&event, &drawn(&view, &description), &view),
                Outcome::Nothing,
            );
        });
    }

    /// #279 and #638: a shortcut over a batched row fills the row and stops.
    /// Apply is what reaches the file, so a patch here would write half the
    /// tab behind the other half.
    #[test]
    fn a_shortcut_over_a_batched_row_stages_rather_than_saves() {
        model::tests::with_env(None, None, None, || {
            let view = director_view();
            let description = form::describe();
            let shortcut = description
                .shortcut(form::DIRECTOR_BASE_URL_PICK_ID)
                .expect("the Base URL row has a shortcut list");
            let title = title_that_writes(&description, form::DIRECTOR_BASE_URL_PICK_ID);
            let event = Event::Shortcut {
                id: form::DIRECTOR_BASE_URL_PICK_ID.into(),
                value: title,
                current: String::new(),
            };
            let Outcome::Fill { id, patch, .. } =
                handle(&event, &drawn(&view, &description), &view)
            else {
                panic!("a shortcut fills the row below it");
            };
            assert_eq!(id, shortcut.row);
            assert!(patch.is_none(), "a batched row waits for Apply");
        });
    }

    /// A title the list cannot spell — Custom — names nothing to write, so the
    /// row below keeps whatever it holds.
    #[test]
    fn a_shortcut_title_that_names_no_value_writes_nothing() {
        model::tests::with_env(None, None, None, || {
            let view = director_view();
            let description = form::describe();
            let event = Event::Shortcut {
                id: form::DIRECTOR_BASE_URL_PICK_ID.into(),
                value: "Custom".into(),
                current: String::new(),
            };
            assert_eq!(
                handle(&event, &drawn(&view, &description), &view),
                Outcome::Nothing,
            );
        });
    }

    /// An id no description carries is not a silent no-op by accident: it is
    /// one on purpose, and the same answer a tag that lost its row gives.
    #[test]
    fn an_unknown_row_does_nothing() {
        model::tests::with_env(None, None, None, || {
            let view = director_view();
            let description = form::describe();
            let draft = drawn(&view, &description);
            for event in [
                press("no_such_row"),
                Event::SetBool {
                    id: "no_such_row".into(),
                    value: true,
                },
                Event::SetText {
                    id: "no_such_row".into(),
                    value: "x".into(),
                },
            ] {
                assert_eq!(handle(&event, &draft, &view), Outcome::Nothing, "{event:?}");
            }
        });
    }

    /// New session carries no row, so a half-typed endpoint beside it survives
    /// the click (#679).
    #[test]
    fn a_new_session_touches_no_row_of_the_form() {
        model::tests::with_env(None, None, None, || {
            let view = director_view();
            let description = form::describe();
            let typed = DirectorDraft {
                base_url: "https://half.typed".into(),
                ..drawn(&view, &description)
            };
            let Outcome::Apply(patch) = handle(&press(form::NEW_SESSION_ID), &typed, &view) else {
                panic!("new session is a patch of its own");
            };
            assert!(patch.new_session);
            assert!(patch.director_base_url.is_none());
        });
    }

    /// The first title on a shortcut list that names a value to write.
    fn title_that_writes(description: &FormDescription, id: &str) -> String {
        let shortcut = description.shortcut(id).expect("a shortcut list");
        description
            .sections()
            .flat_map(|section| section.rows.iter())
            .find_map(|row| match row {
                form::FormRow::Composite { controls, .. } => {
                    controls.iter().find_map(|control| match control {
                        form::CompositeControl::Popup {
                            id: popup, options, ..
                        } if popup == id => options
                            .iter()
                            .find(|title| (shortcut.value)(title).is_some())
                            .cloned(),
                        _ => None,
                    })
                }
                _ => None,
            })
            .expect("at least one title on the list names a value")
    }
}
