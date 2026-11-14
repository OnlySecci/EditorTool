//! Explicit password/recovery interactions; never log secrets or put them in labels.
use crate::*;
use zeroize::Zeroizing;

pub fn password(confirm: bool) -> Option<Zeroizing<String>> {
    Group::set_current(None::<&Group>);
    let mut win = Window::default()
        .with_size(540, if confirm { 290 } else { 210 })
        .center_screen();
    win.set_label("Protected local data — EditorTool");
    win.set_color(PANEL);
    win.make_modal(true);
    chrome::set_icon(&mut win);
    let mut root = Flex::new(22, 18, 496, win.h() - 36, None).column();
    root.set_spacing(10);
    let label = theme::heading(
        if confirm {
            "Create a protection password (at least 12 characters)"
        } else {
            "Unlock protected data"
        },
        16,
    );
    root.fixed(&label, 32);
    let mut first = fltk::input::SecretInput::default();
    first.set_frame(FrameType::FreeBoxType);
    first.set_color(theme::SURFACE);
    first.set_text_color(FG);
    first.set_text_size(16);
    root.fixed(&first, 34);
    let mut second = fltk::input::SecretInput::default();
    second.set_frame(FrameType::FreeBoxType);
    second.set_color(theme::SURFACE);
    second.set_text_color(FG);
    second.set_text_size(16);
    root.fixed(&second, 34);
    if !confirm {
        second.hide();
        root.fixed(&second, 0);
    }
    let mut info = Frame::default().with_label(if confirm {
        "Enter the same password twice. Store it safely; it cannot be recovered."
    } else {
        "Cancelling keeps existing data locked. The editor remains available."
    });
    info.set_label_size(12);
    info.set_label_color(MUTED);
    info.set_align(Align::Left | Align::Inside | Align::Wrap);
    let result = Rc::new(Cell::new(false));
    let row = Flex::default().row();
    let mut cancel = Button::default().with_label("Cancel");
    theme::button(&mut cancel, false, None);
    let mut window = win.clone();
    cancel.set_callback(move |_| window.hide());
    let mut accept = Button::default().with_label(if confirm { "Protect data" } else { "Unlock" });
    theme::button(&mut accept, true, None);
    let accepted = result.clone();
    let a = first.clone();
    let b = second.clone();
    let mut window = win.clone();
    accept.set_callback(move |_| {
        let value = Zeroizing::new(a.value());
        let repeat = Zeroizing::new(b.value());
        if value.is_empty() || confirm && (value.chars().count() < 12 || *value != *repeat) {
            dialog::alert_default("Use matching passwords with at least 12 characters.");
            return;
        }
        accepted.set(true);
        window.hide();
    });
    row.end();
    root.fixed(&row, 36);
    root.end();
    win.end();
    win.set_callback(|w| w.hide());
    win.show();
    chrome::dark_title(&win);
    first.take_focus().ok();
    while win.shown() {
        app::wait();
    }
    let value = result.get().then(|| Zeroizing::new(first.value()));
    first.set_value("");
    second.set_value("");
    Window::delete(win);
    value
}
