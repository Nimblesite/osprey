//! The live-attach wire messages [PROF-ATTACH-PROTOCOL]: one JSON object per
//! line, tagged by `kind`. Only the fields this runtime reads are declared;
//! serde ignores the rest, so the endpoint may add fields freely.

use osprey_profiler::Image;
use serde::Deserialize;

/// The protocol revision this runtime speaks.
pub(crate) const PROTOCOL: u32 = 1;

/// Any line the endpoint sends.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Message {
    /// The reply to `hello`.
    Hello(Hello),
    /// A session started.
    CpuStarted(CpuStarted),
    /// New thread rows, stacks and samples.
    CpuBatch(CpuBatch),
    /// The session ended.
    CpuStopped(CpuStopped),
    /// A request failed.
    Error(EndpointError),
}

/// Who answers on the socket.
#[derive(Debug, Deserialize)]
pub(crate) struct Hello {
    pub(crate) protocol: u32,
    pub(crate) runtime_version: String,
    pub(crate) exe: String,
}

/// The program's images when the session started, for symbolization.
#[derive(Debug, Deserialize)]
pub(crate) struct CpuStarted {
    pub(crate) images: Vec<Image>,
}

/// One thread identity a session's samples reference.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ThreadRow {
    pub(crate) fiber: i64,
    pub(crate) label: String,
}

/// `[t_rel_ns, thread, stack, state, tick]`.
pub(crate) type SampleRow = (u64, u32, u32, u8, u32);

/// What the session gained since the last batch.
#[derive(Debug, Deserialize)]
pub(crate) struct CpuBatch {
    pub(crate) threads: Vec<ThreadRow>,
    pub(crate) stacks: Vec<Vec<u64>>,
    pub(crate) samples: Vec<SampleRow>,
    /// Present only when the program's image list changed.
    #[serde(default)]
    pub(crate) images: Option<Vec<Image>>,
}

/// Why the session ended: `requested`, `exit` or `disconnected`.
#[derive(Debug, Deserialize)]
pub(crate) struct CpuStopped {
    pub(crate) reason: String,
}

/// A refused request.
#[derive(Debug, Deserialize)]
pub(crate) struct EndpointError {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_the_endpoint_sends_parses() {
        let hello: Message = serde_json::from_str(r#"{"kind":"hello","protocol":1,"pid":7,"platform":"macos-arm64","cpu_session":false,"runtime_version":"0.0.0-dev","exe":"/bin/app"}"#).unwrap();
        assert!(matches!(hello, Message::Hello(Hello { protocol: 1, .. })));
        let batch: Message = serde_json::from_str(r#"{"kind":"cpu_batch","threads":[{"fiber":0,"label":"main"}],"stacks":[[4096,8192]],"samples":[[10,0,0,0,1]]}"#).unwrap();
        let Message::CpuBatch(batch) = batch else {
            panic!("not a batch")
        };
        assert_eq!(
            (
                batch.threads.len(),
                batch.stacks.len(),
                batch.samples.first().copied()
            ),
            (1, 1, Some((10, 0, 0, 0, 1)))
        );
        assert!(batch.images.is_none());
        let started: Message = serde_json::from_str(r#"{"kind":"cpu_started","rate_hz":997,"start_unix_ns":1,"images":[{"path":"/bin/app","base":4096,"slide":0,"text":4096,"text_size":100}]}"#).unwrap();
        assert!(matches!(started, Message::CpuStarted(CpuStarted { images }) if images.len() == 1));
        let stopped: Message = serde_json::from_str(
            r#"{"kind":"cpu_stopped","reason":"exit","dropped":0,"end_unix_ns":2}"#,
        )
        .unwrap();
        assert!(matches!(stopped, Message::CpuStopped(CpuStopped { reason }) if reason == "exit"));
        let error: Message =
            serde_json::from_str(r#"{"kind":"error","code":"busy","message":"full"}"#).unwrap();
        assert!(matches!(error, Message::Error(EndpointError { code, .. }) if code == "busy"));
    }
}
