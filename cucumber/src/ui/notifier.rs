use std::sync::mpsc::Sender;

use eframe::egui::Context;

use crate::ui::Event;

#[derive(Clone)]
pub struct UiNotifier {
    ctx: Context,
    tx: Sender<Event>,
}

impl UiNotifier {
    pub fn new(ctx: Context, tx: Sender<Event>) -> Self {
        Self { ctx, tx }
    }

    pub fn notify(&self, event: Event) {
        self.ctx.request_repaint();
        self.tx.send(event).unwrap();
    }
}
