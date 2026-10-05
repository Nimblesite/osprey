use super::*;

#[test]
fn registers_core_and_polymorphic_builtins() {
    let e = base_env();
    assert!(e.get("print").is_some());
    assert_eq!(e.get("map").unwrap().vars.len(), 2);
    assert_eq!(e.get("await").unwrap().vars.len(), 1);
    assert_eq!(
        builtin_signature("fiberDone").as_deref(),
        Some("fiberDone : (Fiber<t0>) -> int")
    );
    assert_eq!(
        builtin_signature("abs").as_deref(),
        Some("abs : (t0) -> t0")
    );
    assert_eq!(
        builtin_signature("intDiv").as_deref(),
        Some("intDiv : (int, int) -> int")
    );
    assert_eq!(
        builtin_signature("wrapAdd").as_deref(),
        Some("wrapAdd : (int, int) -> int")
    );
}

#[test]
fn process_builtins_match_the_result_returning_runtime() {
    assert_eq!(
        builtin_signature("spawnProcess").as_deref(),
        Some("spawnProcess : (string, (int, int, string) -> Unit) -> Result<int, Error>")
    );
    assert_eq!(
        builtin_signature("awaitProcess").as_deref(),
        Some("awaitProcess : (int) -> int")
    );
    assert_eq!(
        builtin_signature("cleanupProcess").as_deref(),
        Some("cleanupProcess : (int) -> Unit")
    );
}

#[test]
fn network_builtins_match_the_runtime_status_abi() {
    let expected = [
        (
            "writeFile",
            "writeFile : (string, string) -> Result<int, Error>",
        ),
        ("httpCloseClient", "httpCloseClient : (int) -> int"),
        ("httpGet", "httpGet : (int, string, string) -> int"),
        (
            "httpResponseFree",
            "httpResponseFree : (int) -> Result<int, Error>",
        ),
        (
            "httpPost",
            "httpPost : (int, string, string, string) -> int",
        ),
        ("httpPut", "httpPut : (int, string, string, string) -> int"),
        ("httpDelete", "httpDelete : (int, string, string) -> int"),
        (
            "httpListen",
            "httpListen : (int, (string, string, string, string) -> HttpResponse) -> int",
        ),
        ("httpStopServer", "httpStopServer : (int) -> int"),
        ("websocketClose", "websocketClose : (int) -> int"),
        ("jsonFree", "jsonFree : (int) -> Result<int, Error>"),
    ];
    for (name, signature) in expected {
        assert_eq!(
            builtin_signature(name).as_deref(),
            Some(signature),
            "{name}"
        );
    }
}

#[test]
fn public_maps_use_the_runtime_string_key_abi() {
    let expected = [
        ("Map", "Map : () -> Map<string, t0>"),
        (
            "mapSet",
            "mapSet : (Map<string, t0>, string, t0) -> Map<string, t0>",
        ),
        (
            "mapGet",
            "mapGet : (Map<string, t0>, string) -> Result<t0, Error>",
        ),
        ("mapKeys", "mapKeys : (Map<string, t0>) -> List<string>"),
    ];
    for (name, signature) in expected {
        assert_eq!(
            builtin_signature(name).as_deref(),
            Some(signature),
            "{name}"
        );
    }
}

#[test]
fn iterator_builtins_do_not_advertise_runtime_lists() {
    let expected = [
        ("range", "range : (int, int) -> Iterator<int>"),
        ("map", "map : (Iterator<t0>, (t0) -> t1) -> Iterator<t1>"),
        (
            "filter",
            "filter : (Iterator<t0>, (t0) -> bool) -> Iterator<t0>",
        ),
        ("forEach", "forEach : (Iterator<t0>, (t0) -> Unit) -> Unit"),
        ("fold", "fold : (Iterator<t0>, t1, (t1, t0) -> t1) -> t1"),
    ];
    for (name, signature) in expected {
        assert_eq!(
            builtin_signature(name).as_deref(),
            Some(signature),
            "{name}"
        );
    }
}
