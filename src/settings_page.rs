//! Reparents a settings surface into a normal document tab without opening a native window.
use crate::*;
pub struct Page {
    pub scroll: Scroll,
    content: Group,
    root: Flex,
    min_height: i32,
    min_width: i32,
}
impl Page {
    pub fn new(win: &mut Window, root: &Flex) -> Self {
        Group::set_current(None::<&Group>);
        win.begin();
        let mut scroll = Scroll::new(0, 0, win.w(), win.h(), None);
        scroll.set_color(PANEL);
        scroll.set_frame(FrameType::FlatBox);
        scroll.set_scrollbar_size(14);
        scroll.super_draw_first(true);
        scroll.draw(|w| theme::draw_scrollbars(w));
        let grab = Cell::new(None);
        scroll.handle(move |w, e| theme::handle_scrollbars(w, e, &grab));
        let mut content = Group::new(0, 0, win.w(), win.h(), None);
        content.set_color(PANEL);
        content.set_frame(FrameType::FlatBox);
        content.end();
        scroll.end();
        win.end();
        content.add(root);
        let mut page = Self {
            scroll,
            content,
            root: root.clone(),
            min_height: root.h() + 40,
            min_width: root.w() + 40,
        };
        page.scroll.hide();
        page
    }
    pub fn park(&mut self, win: &mut Window) {
        if self
            .scroll
            .parent()
            .is_some_and(|p| p.as_widget_ptr() != win.as_widget_ptr())
        {
            win.add(&self.scroll);
        }
        self.scroll.hide();
    }
    pub fn mount(&mut self, parent: &mut Flex) {
        if self
            .scroll
            .parent()
            .is_none_or(|p| p.as_widget_ptr() != parent.as_widget_ptr())
        {
            parent.add(&self.scroll);
            self.scroll.show();
            parent.recalc();
        } else if !self.scroll.visible() {
            self.scroll.show();
            parent.recalc();
        }
        let (x, y, w, h) = (parent.x(), parent.y(), parent.w(), parent.h());
        if (
            self.scroll.x(),
            self.scroll.y(),
            self.scroll.w(),
            self.scroll.h(),
        ) != (x, y, w, h)
        {
            self.scroll.resize(x, y, w, h);
        }
        let width = (w - 14).max(self.min_width);
        let height = h.max(self.min_height);
        self.scroll.set_type(if w < self.min_width {
            ScrollType::Both
        } else if h < self.min_height {
            ScrollType::Vertical
        } else {
            ScrollType::None
        });
        let sy = self.scroll.yposition().clamp(0, (height - h).max(0));
        let sx = self.scroll.xposition().clamp(0, (width - w).max(0));
        if (sx, sy) != (self.scroll.xposition(), self.scroll.yposition()) {
            self.scroll.scroll_to(sx, sy);
        }
        let rect = (x - sx, y - sy, width, height);
        if (
            self.content.x(),
            self.content.y(),
            self.content.w(),
            self.content.h(),
        ) != rect
        {
            self.content.resize(rect.0, rect.1, rect.2, rect.3);
            self.root
                .resize(rect.0 + 20, rect.1 + 20, width - 40, self.min_height - 40);
            self.root.recalc();
            self.content.init_sizes();
            self.scroll.redraw();
        }
    }
}
impl Runtime {
    pub fn park_settings_pages(&mut self) {
        self.clients.page.park(&mut self.clients.win);
        self.keyboard.page.park(&mut self.keyboard.win);
        self.storage_panel.page.park(&mut self.storage_panel.win);
        self.permissions_panel
            .page
            .park(&mut self.permissions_panel.win);
        self.traffic_panel.page.park(&mut self.traffic_panel.win);
        for w in self.windows.iter_mut().flatten() {
            w.settings.page.park(&mut w.settings.win);
        }
    }
    pub fn mount_settings_pages(&mut self) {
        if let Some(w) = self
            .windows
            .iter_mut()
            .flatten()
            .find(|w| w.docs[w.active].page == 6)
        {
            self.traffic_panel.page.mount(&mut w.docs[w.active].group);
        } else {
            self.traffic_panel.page.park(&mut self.traffic_panel.win);
        }
        for page in [4, 5] {
            let panel = if page == 4 {
                &mut self.storage_panel
            } else {
                &mut self.permissions_panel
            };
            if let Some(w) = self
                .windows
                .iter_mut()
                .flatten()
                .find(|w| w.docs[w.active].page == page)
            {
                panel.page.mount(&mut w.docs[w.active].group);
            } else {
                panel.page.park(&mut panel.win);
            }
        }
        for w in self.windows.iter_mut().flatten() {
            if w.docs[w.active].page == 1 {
                w.settings.page.mount(&mut w.docs[w.active].group);
            } else {
                w.settings.page.park(&mut w.settings.win);
            }
        }
        if let Some(w) = self
            .windows
            .iter_mut()
            .flatten()
            .find(|w| w.docs[w.active].page == 2)
        {
            self.clients.page.mount(&mut w.docs[w.active].group);
        } else {
            self.clients.page.park(&mut self.clients.win);
        }
        if let Some(w) = self
            .windows
            .iter_mut()
            .flatten()
            .find(|w| w.docs[w.active].page == 3)
        {
            self.keyboard.page.mount(&mut w.docs[w.active].group);
        } else {
            self.keyboard.page.park(&mut self.keyboard.win);
        }
    }
    pub fn open_keyboard_tab(&mut self, wid: usize) {
        if self.windows.iter().all(Option::is_none) {
            self.handle(0, Action::NewWindow);
        }
        let existing = self.windows.iter().enumerate().find_map(|(wi, w)| {
            w.as_ref()
                .and_then(|w| w.docs.iter().position(|d| d.page == 3).map(|di| (wi, di)))
        });
        let wi = if let Some((wi, di)) = existing {
            self.windows[wi]
                .as_mut()
                .unwrap()
                .action(Action::Select(di), wi, self.tx);
            wi
        } else {
            let wi = if self.windows.get(wid).is_some_and(Option::is_some) {
                wid
            } else {
                self.windows.iter().position(Option::is_some).unwrap()
            };
            self.windows[wi]
                .as_mut()
                .unwrap()
                .open_settings_tab(3, wi, self.tx);
            wi
        };
        let w = self.windows[wi].as_mut().unwrap();
        if !w.embedded {
            w.win.show();
            chrome::dark_title(&w.win);
            shutdown::hook(&w.win, self.weak_self.clone());
        }
    }
    pub fn open_clients_tab(&mut self, wid: usize) {
        let existing = self.windows.iter().enumerate().find_map(|(wi, w)| {
            w.as_ref()
                .and_then(|w| w.docs.iter().position(|d| d.page == 2).map(|di| (wi, di)))
        });
        let wi = if let Some((wi, di)) = existing {
            self.windows[wi]
                .as_mut()
                .unwrap()
                .action(Action::Select(di), wi, self.tx);
            wi
        } else {
            if self.windows.iter().all(Option::is_none) {
                self.handle(0, Action::NewWindow);
            }
            let wi = if self.windows.get(wid).is_some_and(Option::is_some) {
                wid
            } else {
                self.windows.iter().position(Option::is_some).unwrap()
            };
            self.windows[wi]
                .as_mut()
                .unwrap()
                .open_settings_tab(2, wi, self.tx);
            wi
        };
        let w = self.windows[wi].as_mut().unwrap();
        if !w.embedded {
            w.win.show();
            chrome::dark_title(&w.win);
            shutdown::hook(&w.win, self.weak_self.clone());
        }
    }
}
