//! [`OspreyEngine`] — the [`lspkit::EngineApi`] implementation.
//!
//! State is the open-document [`Vfs`] plus a [`Session`] (from `lspkit-live`)
//! that owns the monotonic generation counter and broadcasts change events.
//! Every analysis is recomputed from current document text via [`crate::model`]
//! queries, so the same engine can later back an MCP surface unchanged.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use lspkit::{
    Cause, EngineApi, Generation, GenerationEventStream, Progress, RescanScope, RescanTicket,
    Snapshot,
};
use lspkit_live::Session;
use lspkit_vfs::{PositionEncoding, Vfs};
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;

use crate::analysis::collect_symbols;
use crate::diagnostics;
use crate::features;
use crate::model::{At, EngineError, Query, Report};

/// The Osprey analysis engine.
#[derive(Debug, Clone)]
pub struct OspreyEngine {
    vfs: Vfs,
    session: Session,
    shutdown: Arc<AtomicBool>,
}

impl OspreyEngine {
    /// New engine over `vfs`, starting at [`Generation::ZERO`].
    #[must_use]
    pub fn new(vfs: Vfs) -> Self {
        Self {
            vfs,
            session: Session::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The shared open-document store.
    #[must_use]
    pub fn vfs(&self) -> &Vfs {
        &self.vfs
    }

    fn encoding(&self) -> PositionEncoding {
        self.vfs.encoding()
    }

    fn text(&self, uri: &lspkit_vfs::DocumentUri) -> String {
        self.vfs.text(uri).unwrap_or_default()
    }

    /// Compute the report for `query` against current document state.
    fn answer(&self, query: Query) -> Report {
        let enc = self.encoding();
        match query {
            Query::Diagnostics(uri) => {
                Report::Diagnostics(diagnostics::compute(&self.text(&uri), uri.as_str(), enc))
            }
            Query::Symbols(uri) => {
                let parsed = osprey_syntax::parse_program_for_path(uri.as_str(), &self.text(&uri));
                Report::Symbols(collect_symbols(&parsed.program))
            }
            Query::Hover(at) => Report::Hover(self.hover(&at)),
            Query::Definition(at) => Report::Locations(self.locate(&at, true, false)),
            Query::References {
                at,
                include_declaration,
            } => Report::Locations(self.locate(&at, false, include_declaration)),
            Query::SignatureHelp(at) => Report::Signature(self.signature(&at)),
            Query::Completion(at) => Report::Completion(crate::complete::completion(
                &self.text(&at.uri),
                at.uri.as_str(),
                at.line,
                at.character,
                enc,
            )),
        }
    }

    fn hover(&self, at: &At) -> Option<String> {
        crate::hover::hover(
            &self.text(&at.uri),
            at.uri.as_str(),
            at.line,
            at.character,
            self.encoding(),
        )
    }

    fn signature(&self, at: &At) -> Option<crate::model::SignatureInfo> {
        features::signature_help(
            &self.text(&at.uri),
            at.uri.as_str(),
            at.line,
            at.character,
            self.encoding(),
        )
    }

    fn locate(
        &self,
        at: &At,
        definition: bool,
        include_declaration: bool,
    ) -> Vec<crate::model::Location> {
        let text = self.text(&at.uri);
        let uri = at.uri.as_str();
        if definition {
            features::definition(&text, uri, at.line, at.character, self.encoding())
        } else {
            features::references(
                &text,
                uri,
                at.line,
                at.character,
                self.encoding(),
                include_declaration,
            )
        }
    }
}

#[async_trait]
impl EngineApi for OspreyEngine {
    type Report = Report;
    type Query = Query;
    type Error = EngineError;

    fn generation(&self) -> Generation {
        self.session.generation()
    }

    async fn report(
        &self,
        query: Self::Query,
        _cancel: CancellationToken,
    ) -> Result<Snapshot<Self::Report>, Self::Error> {
        if self.shutdown.load(Ordering::SeqCst) {
            return Err(EngineError::ShuttingDown);
        }
        let generation = self.session.generation();
        Ok(Snapshot::new(generation, self.answer(query)))
    }

    async fn rescan(
        &self,
        _scope: RescanScope,
        _progress: Progress,
    ) -> Result<RescanTicket, Self::Error> {
        if self.shutdown.load(Ordering::SeqCst) {
            return Err(EngineError::ShuttingDown);
        }
        Ok(RescanTicket::new(self.session.advance(Cause::Rescan)))
    }

    fn subscribe(&self) -> GenerationEventStream {
        let rx = self.session.subscribe();
        Box::pin(futures_util::stream::unfold(rx, |mut rx| async move {
            loop {
                match rx.recv().await {
                    Ok(event) => return Some((event, rx)),
                    Err(RecvError::Closed) => return None,
                    // Dropped events: keep waiting for the next live one.
                    Err(RecvError::Lagged(_)) => {}
                }
            }
        }))
    }

    async fn shutdown(&self) -> Result<(), Self::Error> {
        self.shutdown.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lspkit_vfs::{DocumentUri, DocumentVersion};

    fn engine_with(text: &str) -> (OspreyEngine, DocumentUri) {
        let vfs = Vfs::new(PositionEncoding::Utf16);
        let uri = DocumentUri::new("file:///a.osp");
        vfs.open(uri.clone(), text, DocumentVersion::new(1));
        (OspreyEngine::new(vfs), uri)
    }

    #[tokio::test]
    async fn report_answers_symbols_and_advances_on_rescan() {
        let (engine, uri) = engine_with("fn main() -> Unit = print(\"hi\")\n");
        let snap = engine
            .report(Query::Symbols(uri), CancellationToken::new())
            .await
            .expect("report");
        match snap.data {
            Report::Symbols(syms) => assert!(syms.iter().any(|s| s.name == "main")),
            other => panic!("expected symbols, got {other:?}"),
        }
        assert_eq!(engine.generation(), Generation::ZERO);
        let _ticket = engine
            .rescan(RescanScope::All, Progress::noop())
            .await
            .expect("rescan");
        assert_eq!(engine.generation(), Generation::ZERO.next());
    }

    #[tokio::test]
    async fn shutdown_makes_further_calls_fail() {
        let (engine, uri) = engine_with("fn main() = 1\n");
        engine.shutdown().await.expect("shutdown");
        let err = engine
            .report(Query::Diagnostics(uri), CancellationToken::new())
            .await;
        assert!(matches!(err, Err(EngineError::ShuttingDown)));
        // `rescan` is likewise refused after shutdown.
        let rescan = engine.rescan(RescanScope::All, Progress::noop()).await;
        assert!(matches!(rescan, Err(EngineError::ShuttingDown)));
    }

    #[tokio::test]
    async fn report_answers_every_positional_query_kind() {
        use crate::model::{At, Report};
        let src = "fn add(a: int, b: int) -> int = a + b\nlet total = add(1, 2)\n";
        let (engine, uri) = engine_with(src);
        let at = |line, character| At {
            uri: uri.clone(),
            line,
            character,
        };
        let cancel = || CancellationToken::new();
        let report = |q| async { engine.report(q, cancel()).await.expect("report").data };

        // Hover over the call to `add` renders its signature.
        match report(Query::Hover(at(1, 13))).await {
            Report::Hover(Some(md)) => assert!(md.contains("fn add"), "{md}"),
            other => panic!("expected hover, got {other:?}"),
        }
        // Definition lands on the declaration (line 0).
        match report(Query::Definition(at(1, 13))).await {
            Report::Locations(locs) => assert_eq!(locs.first().map(|l| l.span.0), Some(0)),
            other => panic!("expected locations, got {other:?}"),
        }
        // References including the declaration: two occurrences of `add`.
        let refs = report(Query::References {
            at: at(0, 3),
            include_declaration: true,
        })
        .await;
        match refs {
            Report::Locations(locs) => assert_eq!(locs.len(), 2, "{locs:?}"),
            other => panic!("expected locations, got {other:?}"),
        }
        // Signature help over the second argument tracks the active parameter.
        match report(Query::SignatureHelp(at(1, 19))).await {
            Report::Signature(Some(sig)) => {
                assert_eq!(sig.active_parameter, 1);
                assert_eq!(sig.parameters.len(), 2);
            }
            other => panic!("expected signature, got {other:?}"),
        }
        // Completion lists keywords plus the document's own declarations.
        match report(Query::Completion(at(2, 0))).await {
            Report::Completion(items) => {
                assert!(items.iter().any(|i| i.label == "fn"));
                assert!(items.iter().any(|i| i.label == "add"));
            }
            other => panic!("expected completion, got {other:?}"),
        }
        // Diagnostics on a clean program are empty.
        match report(Query::Diagnostics(uri.clone())).await {
            Report::Diagnostics(diags) => assert!(diags.is_empty(), "{diags:?}"),
            other => panic!("expected diagnostics, got {other:?}"),
        }
        // The vfs getter exposes the shared store.
        assert_eq!(engine.vfs().text(&uri).as_deref(), Some(src));
    }

    #[tokio::test]
    async fn subscribe_streams_generation_events_until_closed() {
        use futures_util::StreamExt as _;
        let (engine, _uri) = engine_with("fn main() = 1\n");
        let mut stream = engine.subscribe();
        // A rescan advances the generation and broadcasts an event.
        let _ticket = engine
            .rescan(RescanScope::All, Progress::noop())
            .await
            .expect("rescan");
        let event = stream.next().await.expect("generation event");
        assert_eq!(event.generation, Generation::ZERO.next(), "{event:?}");
        // Dropping the engine closes the broadcast channel, ending the stream.
        drop(engine);
        assert!(stream.next().await.is_none(), "stream ends on close");
    }
}
