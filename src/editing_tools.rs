//! Local tab organisation and explicit, undoable editing commands.
use crate::*;
pub fn prompt(owner: &Window, title: &str, initial: &str) -> Option<String> {
    Group::set_current(None::<&Group>);
    let mut win = Window::default()
        .with_size(480, 164)
        .with_label(i18n::tr(title));
    win.set_color(PANEL);
    win.make_modal(true);
    chrome::set_icon(&mut win);
    let mut root = Flex::new(20, 16, 440, 132, None).column();
    root.set_spacing(10);
    let heading = theme::heading(title, 16);
    root.fixed(&heading, 26);
    let mut input = fltk::input::Input::default();
    theme::input(&mut input);
    input.set_maximum_size(48);
    input.set_tooltip(i18n::tr(
        "Use the same name to group tabs together. Pinned tabs appear first.",
    ));
    input.set_value(initial);
    root.fixed(&input, 32);
    let mut row = Flex::default().row();
    row.set_spacing(8);
    let result = Rc::new(Cell::new(false));
    let mut cancel = Button::default().with_label(i18n::tr("Cancel"));
    theme::button(&mut cancel, false, None);
    let mut close = win.clone();
    cancel.set_callback(move |_| close.hide());
    let mut accept = Button::default().with_label(i18n::tr("Apply"));
    theme::button(&mut accept, true, None);
    let selected = result.clone();
    let mut close = win.clone();
    accept.set_callback(move |_| {
        selected.set(true);
        close.hide();
    });
    row.end();
    root.fixed(&row, 34);
    root.end();
    win.end();
    win.set_pos(
        owner.x() + (owner.w() - win.w()) / 2,
        owner.y() + (owner.h() - win.h()) / 2,
    );
    win.show();
    let _ = input.take_focus();
    while win.shown() {
        app::wait();
    }
    let value = result.get().then(|| {
        input
            .value()
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(48)
            .collect()
    });
    Window::delete(win);
    value
}
impl EditorWindow {
    pub(super) fn tab_order(&self) -> Vec<usize> {
        let mut order: Vec<_> = (0..self.docs.len()).collect();
        order.sort_by_key(|&i| {
            let d = &self.docs[i];
            (
                !d.pinned,
                d.tab_group.is_empty(),
                d.tab_group.to_lowercase(),
                i,
            )
        });
        order
    }
    pub(super) fn tab_label(&self, index: usize) -> String {
        let d = &self.docs[index];
        format!(
            "{}{}{}",
            if d.pinned { "◆ " } else { "" },
            if d.tab_group.is_empty() {
                String::new()
            } else {
                format!("[{}] ", d.tab_group)
            },
            d.title()
        )
    }
    pub(super) fn editing_action(&mut self, action: Action) {
        let groups = self
            .docs
            .iter()
            .map(|d| d.tab_group.clone())
            .collect::<Vec<_>>();
        let d = &mut self.docs[self.active];
        match action {
            Action::TogglePin => {
                d.pinned = !d.pinned;
                self.reveal_tab = true;
            }
            Action::SetTabGroup => {
                if let Some(group) =
                    prompt(&self.win, "Tab group (empty removes group)", &d.tab_group)
                {
                    d.tab_group = groups
                        .iter()
                        .find(|g| g.to_lowercase() == group.to_lowercase())
                        .cloned()
                        .unwrap_or(group);
                    self.reveal_tab = true;
                }
            }
            Action::ClearTabGroup => {
                d.tab_group.clear();
                self.reveal_tab = true;
            }
            Action::ToggleInvisibles if d.page == 0 => {
                let show = !d.left.invisibles();
                d.left.set_invisibles(show);
                d.right.set_invisibles(show);
                d.report = i18n::tr(if show {
                    "Invisible characters shown · Contents unchanged"
                } else {
                    "Invisible characters hidden"
                })
                .into();
            }
            Action::CleanupText if d.page == 0 => {
                let choice=chrome::decision(Some(&self.win),"Clean up text","Choose a cleanup operation for this editor. Images remain in place; Undo restores the previous text.","Trailing spaces: remove spaces/tabs at line ends. Empty lines: keep at most one consecutive empty line. Both applies both operations. Indentation is preserved.",&["Cancel","Trailing spaces","Empty lines","Both"],0);
                if let Some(choice @ 1..=3) = choice {
                    let editor = if d.target_right.get() {
                        &mut d.right
                    } else {
                        &mut d.left
                    };
                    let changed = editor.cleanup(choice != 2, choice != 1);
                    d.report = i18n::tr(if changed {
                        "Text cleaned · Undo restores the previous text"
                    } else {
                        "No cleanup needed"
                    })
                    .into();
                }
            }
            _ => (),
        }
        self.tab_signature.clear();
    }
}
