//! Work-done progress for diagnostics checked beside requests
//! [LSP-DIAGNOSTICS-SCOPE]: the client shows "Checking Osprey files" while
//! edited files and the repository sweep are checked, and hears when all of
//! them are published.

use super::{Context, Message, RequestId};
use serde_json::{json, Value};

const TOKEN: &str = "osprey/diagnostics";
const TITLE: &str = "Checking Osprey files";

/// Whether a sweep's progress is showing, and how many files it started with.
#[derive(Default)]
pub(super) struct Sweep {
    total: Option<usize>,
    created: bool,
    shown: usize,
}

impl Sweep {
    /// Begin, report or end the progress to match the files left.
    pub(super) async fn show(&mut self, context: &Context) {
        let (enabled, left) = {
            let scope = super::super::scope::lock(&context.engine);
            (scope.reports_progress(), scope.sweep_left())
        };
        match (enabled, self.total, left) {
            (false, _, _) | (true, None, 0) => {}
            (true, None, _) => self.begin(context, left).await,
            (true, Some(_), 0) => self.end(context).await,
            (true, Some(total), _) => self.report(context, total, left).await,
        }
    }

    async fn begin(&mut self, context: &Context, left: usize) {
        if !self.created {
            self.created = true;
            let create = json!({ "token": TOKEN });
            let request = Message::request(
                RequestId::String(TOKEN.into()),
                "window/workDoneProgress/create",
                create,
            );
            let _ = context.writer.write_message(&request).await;
        }
        self.total = Some(left);
        self.shown = 0;
        let value =
            json!({ "kind": "begin", "title": TITLE, "percentage": 0, "message": files(left) });
        progress(context, value).await;
    }

    /// Report only when the percentage moves, so a large sweep stays quiet.
    async fn report(&mut self, context: &Context, total: usize, left: usize) {
        let done = total.saturating_sub(left);
        let percentage = done * 100 / total.max(1);
        if percentage > self.shown {
            self.shown = percentage;
            let value =
                json!({ "kind": "report", "percentage": percentage, "message": files(left) });
            progress(context, value).await;
        }
    }

    async fn end(&mut self, context: &Context) {
        self.total = None;
        progress(context, json!({ "kind": "end" })).await;
    }
}

async fn progress(context: &Context, value: Value) {
    let message = Message::notification("$/progress", json!({ "token": TOKEN, "value": value }));
    let _ = context.writer.write_message(&message).await;
}

fn files(left: usize) -> String {
    format!("{left} files left")
}
