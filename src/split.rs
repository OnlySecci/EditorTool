use crate::*;

pub struct Host {
    pub first: usize,
    pub second: usize,
    panels: Flex,
    divider: Frame,
    stacked: Rc<Cell<bool>>,
    ratio: Rc<Cell<i32>>,
    geometry: Option<(i32, i32, bool, i32)>,
}

impl EditorWindow {
    pub fn wire_root(&mut self, wid: usize) {
        let route = self.route.clone();
        self.root.handle(move |root, event| {
            if event == Event::Push
                && app::event_x() >= root.x()
                && app::event_x() < root.x() + root.w()
                && app::event_y() >= root.y()
                && app::event_y() < root.y() + root.h()
            {
                if route.replace(wid) != wid {
                    if let Some(mut win) = root.window() {
                        win.redraw();
                    }
                }
            }
            false
        });
        let route = self.route.clone();
        self.root.super_draw_first(true);
        self.root.draw(move |root| {
            if route.get() == wid {
                fltk::draw::set_draw_color(theme::BLUE);
                fltk::draw::draw_line(root.x(), root.y(), root.x() + root.w() - 1, root.y());
            }
        });
    }
    pub fn follow_focus(&mut self, wid: usize) {
        let mut focus = app::focus().map(|w| w.as_base_widget());
        while let Some(widget) = focus {
            if widget.as_widget_ptr() == self.root.as_widget_ptr() {
                if self.route.replace(wid) != wid {
                    self.win.redraw();
                }
                break;
            }
            focus = widget.parent().map(|p| p.as_base_widget());
        }
    }
}

impl Runtime {
    pub fn move_tab(&mut self, wid: usize, index: usize) {
        if self
            .windows
            .get(wid)
            .and_then(Option::as_ref)
            .is_none_or(|w| index >= w.docs.len() || w.docs[index].page != 0)
        {
            return;
        }
        if !self
            .splits
            .iter()
            .any(|h| h.first == wid || h.second == wid)
        {
            self.open_split(wid, false);
        }
        let Some(host) = self
            .splits
            .iter()
            .find(|h| h.first == wid || h.second == wid)
        else {
            return;
        };
        let peer = if host.first == wid {
            host.second
        } else {
            host.first
        };
        let mut source = self.windows[wid].take().unwrap();
        let mut target = self.windows[peer].take().unwrap();
        let mut document = source.docs.remove(index);
        source.area.remove(&document.group);
        if source.docs.is_empty() {
            source.active = 0;
            source.add_tab(wid, self.tx);
        } else {
            source.active = source.active.min(source.docs.len() - 1);
            source.docs[source.active].group.show();
        }
        source.tab_signature.clear();
        source.area.resizable(&source.docs[source.active].group);
        source.reveal_tab = true;
        if target.docs.len() == 1 {
            let blank = &target.docs[0];
            if blank.page == 0
                && blank.sharing.is_none()
                && blank.right_path.is_none()
                && !blank.dirty()
                && blank.path.is_none()
                && blank.left_buf.length() == 0
                && blank.right_buf.length() == 0
            {
                let blank = target.docs.remove(0);
                Flex::delete(blank.group);
            }
        }
        for d in &mut target.docs {
            d.group.hide();
        }
        document.id = target.next_doc;
        target.next_doc += 1;
        document.syntax_pending = false;
        document.clear_styles();
        document.rewire_comparison(peer, self.tx);
        target.area.add(&document.group);
        target.area.resizable(&document.group);
        document.group.show();
        target.docs.push(document);
        target.active = target.docs.len() - 1;
        target.tab_signature.clear();
        target.reveal_tab = true;
        target.route.set(peer);
        target.docs[target.active].left.take_focus().ok();
        self.windows[wid] = Some(source);
        self.windows[peer] = Some(target);
    }
    pub fn open_split(&mut self, wid: usize, stacked: bool) {
        if let Some(host) = self
            .splits
            .iter_mut()
            .find(|s| s.first == wid || s.second == wid)
        {
            host.stacked.set(stacked);
            host.geometry = None;
            self.layout_splits();
            return;
        }
        if self.windows.get(wid).is_none_or(Option::is_none) {
            return;
        }
        let peer = self.windows.len();
        self.windows.push(Some(EditorWindow::build(
            peer,
            self.tx,
            self.vertical,
            false,
        )));
        self.join_panes(wid, peer, stacked, 500);
    }
    fn join_panes(&mut self, first: usize, second: usize, stacked: bool, ratio: i32) {
        if first == second
            || self.windows.get(first).is_none_or(Option::is_none)
            || self.windows.get(second).is_none_or(Option::is_none)
        {
            return;
        }
        let mut a = self.windows[first].take().unwrap();
        let mut b = self.windows[second].take().unwrap();
        let key = a.split_key.clone().unwrap_or_else(session::new_key);
        let direction = Rc::new(Cell::new(stacked));
        let ratio = Rc::new(Cell::new(ratio.clamp(200, 800)));
        a.win.remove(&a.root);
        b.win.hide();
        b.win.remove(&b.root);
        let old_window = b.win.clone();
        b.win = a.win.clone();
        b.embedded = true;
        b.route = a.route.clone();
        Window::delete(old_window);
        a.split_key = Some(key.clone());
        b.split_key = Some(key);
        a.split_stacked = direction.clone();
        b.split_stacked = direction.clone();
        a.split_ratio = ratio.clone();
        b.split_ratio = ratio.clone();
        b.wire_root(second);
        a.win.begin();
        let mut panels = Flex::new(0, 0, a.win.w(), a.win.h(), None).row();
        panels.set_spacing(0);
        panels.set_color(PANEL);
        panels.add(&a.root);
        let mut divider = Frame::default();
        panels.add(&b.root);
        panels.end();
        a.win.end();
        a.win.resizable(&panels);
        let layout = panels.clone();
        let axis = direction.clone();
        let value = ratio.clone();
        let tx = self.tx;
        divider.set_tooltip("Ziehen, um die Grösse der Ansichten zu ändern");
        divider.handle(move |frame, event| match event {
            Event::Enter => {
                if let Some(mut win) = frame.window() {
                    win.set_cursor(if axis.get() {
                        fltk::enums::Cursor::NS
                    } else {
                        fltk::enums::Cursor::WE
                    });
                }
                true
            }
            Event::Leave => {
                if let Some(mut win) = frame.window() {
                    win.set_cursor(fltk::enums::Cursor::Default);
                }
                true
            }
            Event::Push | Event::Drag => {
                let position = if axis.get() {
                    app::event_y() - layout.y()
                } else {
                    app::event_x() - layout.x()
                };
                let length = if axis.get() { layout.h() } else { layout.w() };
                value.set((position * 1000 / length.max(1)).clamp(200, 800));
                tx.send((first, Action::RelayoutSplit));
                true
            }
            Event::Released => true,
            _ => false,
        });
        let axis = direction.clone();
        divider.draw(move |frame| {
            fltk::draw::set_draw_color(PANEL);
            fltk::draw::draw_rectf(frame.x(), frame.y(), frame.w(), frame.h());
            fltk::draw::set_draw_color(theme::BORDER);
            if axis.get() {
                fltk::draw::draw_line(
                    frame.x() + 8,
                    frame.y() + 3,
                    frame.x() + frame.w() - 8,
                    frame.y() + 3,
                );
            } else {
                fltk::draw::draw_line(
                    frame.x() + 3,
                    frame.y() + 8,
                    frame.x() + 3,
                    frame.y() + frame.h() - 8,
                );
            }
        });
        Group::set_current(None::<&Group>);
        self.windows[first] = Some(a);
        self.windows[second] = Some(b);
        self.splits.push(Host {
            first,
            second,
            panels,
            divider,
            stacked: direction,
            ratio,
            geometry: None,
        });
        self.layout_splits();
    }
    pub fn layout_splits(&mut self) {
        for host in &mut self.splits {
            let stacked = host.stacked.get();
            let ratio = host.ratio.get();
            let geometry = (host.panels.w(), host.panels.h(), stacked, ratio);
            if host.geometry == Some(geometry) {
                continue;
            }
            host.geometry = Some(geometry);
            host.panels.set_type(if stacked {
                FlexType::Column
            } else {
                FlexType::Row
            });
            host.panels.fixed(&host.divider, 6);
            let available = (if stacked {
                host.panels.h()
            } else {
                host.panels.w()
            } - 6)
                .max(1);
            let minimum = (available / 2).min(if stacked { 180 } else { 360 }).max(1);
            let extent =
                (available * ratio / 1000).clamp(minimum, (available - minimum).max(minimum));
            if let Some(Some(first)) = self.windows.get(host.first) {
                host.panels.fixed(&first.root, extent);
            }
            host.panels.recalc();
            host.panels.redraw();
        }
    }
    pub fn merge_split(&mut self, wid: usize) {
        let Some(index) = self
            .splits
            .iter()
            .position(|h| h.first == wid || h.second == wid)
        else {
            return;
        };
        let mut host = self.splits.remove(index);
        let mut first = self.windows[host.first].take().unwrap();
        let mut second = self.windows[host.second].take().unwrap();
        host.panels.remove(&first.root);
        host.panels.remove(&second.root);
        first.win.remove(&host.panels);
        Flex::delete(host.panels);
        first.win.add(&first.root);
        first.win.resizable(&first.root);
        let selected = if wid == host.second {
            second.docs[second.active].cache_key.clone()
        } else {
            first.docs[first.active].cache_key.clone()
        };
        for mut document in second.docs.drain(..) {
            second.area.remove(&document.group);
            if document.page != 0 && first.docs.iter().any(|d| d.page == document.page) {
                Flex::delete(document.group);
                continue;
            }
            document.id = first.next_doc;
            first.next_doc += 1;
            document.syntax_pending = false;
            document.rewire_comparison(host.first, self.tx);
            document.group.hide();
            first.area.add(&document.group);
            first.docs.push(document);
        }
        if first.docs.len() > 1 {
            let mut i = 0;
            while i < first.docs.len() {
                let d = &first.docs[i];
                let blank = d.page == 0
                    && !d.dirty()
                    && d.path.is_none()
                    && d.right_path.is_none()
                    && d.sharing.is_none()
                    && d.left_buf.length() == 0
                    && d.right_buf.length() == 0;
                if blank && first.docs.len() > 1 {
                    let d = first.docs.remove(i);
                    Flex::delete(d.group);
                } else {
                    i += 1;
                }
            }
        }
        first.active = first
            .docs
            .iter()
            .position(|d| d.cache_key == selected)
            .unwrap_or(0);
        for d in &mut first.docs {
            d.group.hide();
        }
        first.docs[first.active].group.show();
        first.area.resizable(&first.docs[first.active].group);
        first.split_key = None;
        first.route = Rc::new(Cell::new(host.first));
        first.wire_root(host.first);
        wire_window(&mut first.win, first.route.clone(), self.tx);
        first.root.resize(0, 0, first.win.w(), first.win.h());
        first.root.recalc();
        first.resize_docs();
        first.tab_signature.clear();
        first.reveal_tab = true;
        Window::delete(second.find.win);
        Window::delete(second.settings.win);
        Flex::delete(second.root);
        first.win.redraw();
        self.windows[host.first] = Some(first);
        self.persist();
    }
    pub fn end_split(&mut self, wid: usize) {
        let Some(index) = self
            .splits
            .iter()
            .position(|h| h.first == wid || h.second == wid)
        else {
            return;
        };
        let mut host = self.splits.remove(index);
        let mut first = self.windows[host.first].take().unwrap();
        let mut second = self.windows[host.second].take().unwrap();
        host.panels.remove(&first.root);
        host.panels.remove(&second.root);
        first.win.remove(&host.panels);
        Flex::delete(host.panels);
        first.win.add(&first.root);
        first.win.resizable(&first.root);
        first.root.resize(0, 0, first.win.w(), first.win.h());
        first.root.recalc();
        first.split_key = None;
        first.route = Rc::new(Cell::new(host.first));
        first.wire_root(host.first);
        wire_window(&mut first.win, first.route.clone(), self.tx);
        Group::set_current(None::<&Group>);
        let mut win = Window::default()
            .with_size(first.win.w(), first.win.h())
            .with_pos(first.win.x() + 32, first.win.y() + 32)
            .with_label("EditorTool");
        win.set_color(PANEL);
        win.size_range(800, 480, 0, 0);
        chrome::set_icon(&mut win);
        win.add(&second.root);
        win.resizable(&second.root);
        win.end();
        second.win = win;
        second.embedded = false;
        second.split_key = None;
        second.route = Rc::new(Cell::new(host.second));
        second.root.resize(0, 0, second.win.w(), second.win.h());
        second.root.recalc();
        second.wire_root(host.second);
        wire_window(&mut second.win, second.route.clone(), self.tx);
        second.win.show();
        chrome::dark_title(&second.win);
        shutdown::hook(&second.win, self.weak_self.clone());
        first.win.redraw();
        self.windows[host.first] = Some(first);
        self.windows[host.second] = Some(second);
    }
    pub fn restore_splits(&mut self) {
        let mut groups = std::collections::BTreeMap::<String, Vec<usize>>::new();
        for (id, window) in self.windows.iter().enumerate() {
            if let Some(key) = window.as_ref().and_then(|w| w.split_key.clone()) {
                groups.entry(key).or_default().push(id);
            }
        }
        for ids in groups.values() {
            if ids.len() == 2 {
                let first = self.windows[ids[0]].as_ref().unwrap();
                let stacked = first.split_stacked.get();
                let ratio = first.split_ratio.get();
                self.join_panes(ids[0], ids[1], stacked, ratio);
            } else {
                for &id in ids {
                    if let Some(w) = &mut self.windows[id] {
                        w.split_key = None;
                    }
                }
            }
        }
    }
    pub fn restore_closed(&mut self) {
        let Some(state) = self.closed.last().cloned() else {
            return;
        };
        let mut states = vec![state.clone()];
        if let Some(key) = &state.split_key {
            if let Some(other) = self
                .closed
                .iter()
                .rev()
                .skip(1)
                .find(|s| s.split_key.as_ref() == Some(key))
            {
                states.insert(0, other.clone());
            }
        }
        let mut ids = Vec::new();
        let start = self.windows.len();
        for state in &states {
            let id = self.windows.len();
            let mut w = EditorWindow::build(id, self.tx, self.vertical, false);
            if !w.restore(state, &self.store, id, self.tx) {
                Window::delete(w.find.win);
                Window::delete(w.settings.win);
                Window::delete(w.win);
                for w in self.windows.drain(start..).flatten() {
                    Window::delete(w.find.win);
                    Window::delete(w.settings.win);
                    Window::delete(w.win);
                }
                return;
            }
            ids.push(id);
            self.windows.push(Some(w));
        }
        for _ in &states {
            self.closed.pop();
        }
        if ids.len() == 2 {
            self.join_panes(ids[0], ids[1], state.split_stacked, state.split_ratio);
        }
        for id in ids {
            let w = self.windows[id].as_mut().unwrap();
            if !w.embedded {
                w.win.show();
                chrome::dark_title(&w.win);
                shutdown::hook(&w.win, self.weak_self.clone());
            }
        }
        diagnostics::event("window", "restore_closed");
        self.persist();
    }
    pub fn close_editor_window(&mut self, wid: usize) {
        if self.windows.get(wid).is_none_or(Option::is_none) {
            return;
        }
        let group = self
            .splits
            .iter()
            .position(|h| h.first == wid || h.second == wid);
        let ids = if let Some(i) = group {
            vec![self.splits[i].first, self.splits[i].second]
        } else {
            vec![wid]
        };
        if !self.cache_enabled {
            for &id in &ids {
                if let Some(w) = &mut self.windows[id] {
                    for index in 0..w.docs.len() {
                        if !w.confirm_close(index) {
                            return;
                        }
                    }
                }
            }
        }
        self.persist();
        if self.cache_enabled {
            if let Err(e) = self.store.flush() {
                dialog::alert_default(&format!(
                    "Fenster bleibt offen: Sitzung konnte nicht gesichert werden: {e}"
                ));
                self.store.revisions.clear();
                self.last_fingerprint.clear();
                return;
            }
        }
        if preferences::get().keep_in_tray && self.tray.is_some() {
            for id in ids {
                if let Some(w) = &mut self.windows[id] {
                    w.find.win.hide();
                    w.settings.win.hide();
                    if !w.embedded {
                        w.win.hide();
                    }
                }
            }
            self.clients.win.hide();
            self.transfer.win.hide();
            self.conflicts.win.hide();
            self.updates.win.hide();
            return;
        }
        let win = self.windows[wid].as_ref().unwrap().win.clone();
        let last = self
            .windows
            .iter()
            .enumerate()
            .all(|(id, w)| w.is_none() || ids.contains(&id));
        let states: Vec<_> = ids
            .iter()
            .filter_map(|&id| self.windows[id].as_ref().map(EditorWindow::snapshot))
            .collect();
        if last {
            self.last_closed = states;
        } else {
            self.closed.extend(states);
            while self.closed.len() > 20 {
                let removed = self.closed.remove(0);
                if let Some(key) = removed.split_key {
                    self.closed.retain(|w| w.split_key.as_ref() != Some(&key));
                }
            }
        }
        diagnostics::event("window", if last { "close_last" } else { "close_extra" });
        for id in ids {
            if let Some(w) = self.windows[id].take() {
                Window::delete(w.find.win);
                Window::delete(w.settings.win);
            }
        }
        if let Some(i) = group {
            self.splits.remove(i);
        }
        Window::delete(win);
        self.persist();
        if self.windows.iter().all(Option::is_none) {
            self.updates.win.hide();
        }
    }
}

pub fn wire_window(win: &mut Window, route: Rc<Cell<usize>>, tx: app::Sender<Message>) {
    let close_route = route.clone();
    win.set_callback(move |_| tx.send((close_route.get(), Action::CloseWindow)));
    win.handle(move |_, ev| {
        let wid = route.get();
        if ev == Event::KeyDown {
            if shortcuts::capturing() {
                return false;
            }
            if app::focus().is_some_and(|w| {
                fltk::input::Input::from_dyn_widget(&w).is_some()
                    || fltk::output::Output::from_dyn_widget(&w).is_some()
            }) {
                let key = app::event_key();
                if ['a', 'c', 'v', 'x', 'y', 'z']
                    .iter()
                    .any(|c| key == fltk::enums::Key::from_char(*c))
                    && app::event_state().contains(Shortcut::Command)
                {
                    return false;
                }
            }
            if let Some(command) = shortcuts::resolve() {
                tx.send((wid, command.action()));
                return true;
            }
            if app::event_key() == fltk::enums::Key::Escape {
                return true;
            }
        }
        false
    });
}
