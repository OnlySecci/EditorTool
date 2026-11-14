//! In-window, dismissible status notices. They never steal focus or block editing.
use crate::*;
pub struct Notice {
    group: Group,
    text: Frame,
    queue: std::collections::VecDeque<String>,
}
impl Notice {
    pub fn new(win: &mut Group, route: Rc<Cell<usize>>, tx: app::Sender<Message>) -> Self {
        win.begin();
        let mut group = Group::new(0, 0, 390, 82, None);
        group.set_color(theme::SURFACE);
        group.set_frame(FrameType::BorderBox);
        let mut text = Frame::new(12, 8, 332, 66, None);
        text.set_label_size(13);
        text.set_label_color(FG);
        text.set_align(Align::Left | Align::Inside | Align::Wrap);
        let mut close = Button::new(350, 8, 28, 28, None);
        theme::button(&mut close, false, Some(theme::Icon::Close));
        close.set_tooltip(crate::i18n::tr("Dismiss notice"));
        close.set_callback(move |_| tx.send((route.get(), Action::DismissNotice)));
        group.end();
        win.end();
        group.hide();
        Self {
            group,
            text,
            queue: std::collections::VecDeque::new(),
        }
    }
    pub fn push(&mut self, message: String) {
        if self.queue.back() != Some(&message) {
            if self.queue.len() >= 16 {
                self.queue.pop_front();
            }
            self.queue.push_back(message);
        }
        self.show();
    }
    fn show(&mut self) {
        if let Some(text) = self.queue.front() {
            self.text.set_label(&format!(
                "{}{}",
                text,
                if self.queue.len() > 1 {
                    format!("\n{} weitere Hinweise", self.queue.len() - 1)
                } else {
                    String::new()
                }
            ));
            self.group.show();
        } else {
            self.group.hide();
        }
    }
    pub fn dismiss(&mut self) {
        self.queue.pop_front();
        self.show();
    }
    pub fn layout(&mut self, win: &Group) {
        let width = 390.min((win.w() - 24).max(200));
        let x = win.x() + (win.w() - width - 12).max(0);
        let y = win.y() + (win.h() - 94).max(0);
        if (self.group.x(), self.group.y(), self.group.w()) != (x, y, width) {
            self.group.resize(x, y, width, 82);
            self.text.resize(x + 12, y + 8, width - 58, 66);
            if let Some(mut close) = self.group.child(1) {
                close.resize(x + width - 40, y + 8, 28, 28);
            }
        }
        if !self.queue.is_empty() {
            let mut parent = win.clone();
            if parent
                .child(parent.children() - 1)
                .is_some_and(|w| w.as_widget_ptr() != self.group.as_widget_ptr())
            {
                parent.remove(&self.group);
                parent.add(&self.group);
                parent.redraw();
            }
        }
    }
}
