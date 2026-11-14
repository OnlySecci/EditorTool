use crate::*;
pub struct Checks {
    scroll: Scroll,
    content: Group,
    rows: Vec<fltk::button::CheckButton>,
}
fltk::widget_extends!(Checks, Scroll, scroll);
impl Checks {
    pub fn new() -> Self {
        let mut scroll = Scroll::default();
        scroll.set_color(BG);
        scroll.set_frame(FrameType::FlatBox);
        scroll.set_type(ScrollType::Vertical);
        scroll.set_scrollbar_size(14);
        scroll.super_draw_first(true);
        scroll.draw(|w| theme::draw_scrollbars(w));
        let grab = Cell::new(None);
        scroll.handle(move |w, e| theme::handle_scrollbars(w, e, &grab));
        let mut content = Group::default();
        content.set_color(BG);
        content.end();
        scroll.end();
        let list = Self {
            scroll,
            content,
            rows: Vec::new(),
        };
        list
    }
    pub fn clear(&mut self) {
        self.content.clear();
        self.rows.clear();
        self.scroll.scroll_to(0, 0);
    }
    pub fn add(&mut self, label: &str, value: bool, online: bool) {
        self.content.begin();
        let mut row = fltk::button::CheckButton::default().with_label(label);
        theme::check(&mut row);
        row.set_label_size(13);
        row.set_label_color(if online {
            Color::from_hex(0x49bb75)
        } else {
            Color::from_hex(0xe46666)
        });
        row.set_value(value);
        self.content.end();
        self.rows.push(row);
        self.layout();
    }
    pub fn layout(&mut self) {
        let x = self.scroll.x();
        let y = self.scroll.y() - self.scroll.yposition();
        let width = (self.scroll.w() - 18).max(120);
        self.content.resize(
            x,
            y,
            width,
            (self.rows.len() as i32 * 34).max(self.scroll.h()),
        );
        for (i, row) in self.rows.iter_mut().enumerate() {
            row.resize(x + 6, y + i as i32 * 34, width - 12, 32);
        }
        self.content.init_sizes();
    }
    pub fn checked(&self, index: i32) -> bool {
        self.rows
            .get(index.saturating_sub(1) as usize)
            .is_some_and(|r| r.value())
    }
    pub fn set_checked(&mut self, index: i32) {
        if let Some(row) = self.rows.get_mut(index.saturating_sub(1) as usize) {
            row.set_value(true);
        }
    }
    pub fn statuses(&mut self, peers: &[peer::Peer], service: &peer::Service) {
        for (row, peer) in self.rows.iter_mut().zip(peers) {
            row.set_label(&format!(
                "{} · {}:{} · {}",
                peer.name,
                peer.host,
                peer.port,
                if service.status(peer) {
                    crate::i18n::tr("Connected")
                } else {
                    crate::i18n::tr("Not connected")
                }
            ));
            row.set_label_color(if service.status(peer) {
                Color::from_hex(0x49bb75)
            } else {
                Color::from_hex(0xe46666)
            });
            row.redraw();
        }
    }
}
