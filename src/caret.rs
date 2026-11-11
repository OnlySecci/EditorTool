//! Only the focused editor owns a blink timer. Hidden editors do no idle work.
use fltk::{app, prelude::*, text::TextEditor};
use std::cell::RefCell;
#[derive(Default)]
struct Caret {
    editor: Option<TextEditor>,
    timer: Option<app::TimeoutHandle>,
    visible: bool,
}
thread_local! {static CURRENT:RefCell<Caret>=RefCell::new(Caret::default());}
pub fn stop(editor: &TextEditor) {
    CURRENT.with(|cell| {
        let mut c = cell.borrow_mut();
        if c.editor
            .as_ref()
            .is_some_and(|e| !e.was_deleted() && e.as_widget_ptr() == editor.as_widget_ptr())
        {
            if let Some(t) = c.timer.take() {
                app::remove_timeout3(t);
            }
            if let Some(mut e) = c.editor.take() {
                if !e.was_deleted() {
                    e.show_cursor(false);
                }
            }
        }
    });
}
pub fn reset(editor: &mut TextEditor) {
    CURRENT.with(|cell| {
        let mut c = cell.borrow_mut();
        if let Some(t) = c.timer.take() {
            app::remove_timeout3(t);
        }
        if let Some(mut old) = c.editor.take() {
            if !old.was_deleted() && old.as_widget_ptr() != editor.as_widget_ptr() {
                old.show_cursor(false);
            }
        }
        editor.show_cursor(true);
        c.editor = Some(editor.clone());
        c.visible = true;
        c.timer = Some(app::add_timeout3(0.55, tick));
    });
}
fn tick(_: app::TimeoutHandle) {
    CURRENT.with(|cell| {
        let mut c = cell.borrow_mut();
        c.timer = None;
        let Some(mut editor) = c.editor.clone() else {
            return;
        };
        if editor.was_deleted() {
            c.editor = None;
            return;
        }
        if !editor.visible_r()
            || !app::focus().is_some_and(|w| w.as_widget_ptr() == editor.as_widget_ptr())
        {
            editor.show_cursor(false);
            c.editor = None;
            return;
        }
        c.visible = !c.visible;
        editor.show_cursor(c.visible);
        c.timer = Some(app::add_timeout3(0.55, tick));
    });
}
