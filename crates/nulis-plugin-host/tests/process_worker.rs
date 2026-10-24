use nulis_plugin_host::{
    PluginHostOperation, PluginPackage, PluginWorkerRequest, ProcessPluginRunner,
};
use serde_json::json;

#[test]
fn process_runner_uses_json_only_worker_protocol() {
    let runner = ProcessPluginRunner::new(env!("CARGO_BIN_EXE_nulis-plugin-worker").into());
    let response = runner
        .run(
            &PluginWorkerRequest {
                api_version: 1,
                source: r#"
function activate(nulis, context) {
  context.subscriptions.push(nulis.commands.registerCommand("nulis.example.hello", (name) => {
    nulis.window.showInformationMessage(`Hello ${name}`);
    return name.length;
  }));
}"#
                .into(),
                command: Some("nulis.example.hello".into()),
                arguments: vec![json!("Nulis")],
            },
            || false,
        )
        .expect("process worker");
    assert_eq!(response.result, json!(5));
    assert_eq!(
        response.operations,
        [PluginHostOperation::ShowMessage {
            message: "Hello Nulis".into()
        }]
    );
}

#[test]
fn process_runner_kills_cancelled_plugins() {
    let runner = ProcessPluginRunner::new(env!("CARGO_BIN_EXE_nulis-plugin-worker").into());
    let result = runner.run(
        &PluginWorkerRequest {
            api_version: 1,
            source: "function activate() { while (true) {} }".into(),
            command: None,
            arguments: Vec::new(),
        },
        || true,
    );
    assert!(matches!(
        result,
        Err(nulis_plugin_host::PluginError::Cancelled)
    ));
}

#[test]
fn installable_hello_example_executes_in_the_worker() {
    let package_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/hello-nulis");
    let package = PluginPackage::load(&package_root).expect("load example package");
    let runner = ProcessPluginRunner::new(env!("CARGO_BIN_EXE_nulis-plugin-worker").into());
    let response = runner
        .run(
            &package
                .worker_request(
                    Some("nulis.hello-example.hello".into()),
                    vec![serde_json::Value::Null],
                )
                .expect("entry"),
            || false,
        )
        .expect("execute example package");

    assert_eq!(response.result, serde_json::Value::Null);
    assert_eq!(
        response.operations,
        [PluginHostOperation::ShowMessage {
            message: "Hello from an independently built Nulis plugin.".into()
        }]
    );
}

#[test]
fn installable_tips_example_handles_startup_arguments_and_results() {
    let package_root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/nulis-tips");
    let package = PluginPackage::load(&package_root).expect("load example package");
    let runner = ProcessPluginRunner::new(env!("CARGO_BIN_EXE_nulis-plugin-worker").into());
    let messages = |response: &nulis_plugin_host::PluginWorkerResponse| {
        response
            .operations
            .iter()
            .map(|operation| match operation {
                PluginHostOperation::ShowMessage { message } => message.clone(),
            })
            .collect::<Vec<_>>()
    };

    // Startup activation registers both commands and shows the tip of the day.
    let startup = runner
        .run(
            &package.worker_request(None, Vec::new()).expect("entry"),
            || false,
        )
        .expect("startup activation");
    assert_eq!(
        startup.registered_commands,
        ["nulis.tips.count", "nulis.tips.show"]
    );
    assert!(messages(&startup)[0].starts_with("Tip of the day: "));

    // An argument chooses the tip; the handler's result is returned as JSON.
    let shown = runner
        .run(
            &package
                .worker_request(Some("nulis.tips.show".into()), vec![json!({ "index": 1 })])
                .expect("entry"),
            || false,
        )
        .expect("show a chosen tip");
    assert_eq!(
        shown.result,
        json!({ "index": 1, "text": "Hold Alt and click to add a second caret." })
    );
    assert_eq!(
        messages(&shown).last().map(String::as_str),
        Some("Tip 2 of 8: Hold Alt and click to add a second caret.")
    );

    // The command palette passes an empty object: the tip of the day is used.
    let palette = runner
        .run(
            &package
                .worker_request(Some("nulis.tips.show".into()), vec![json!({})])
                .expect("entry"),
            || false,
        )
        .expect("show the tip of the day");
    assert!(palette.result["index"]
        .as_u64()
        .is_some_and(|index| index < 8));

    let count = runner
        .run(
            &package
                .worker_request(Some("nulis.tips.count".into()), vec![json!({})])
                .expect("entry"),
            || false,
        )
        .expect("count tips");
    assert_eq!(count.result, json!(8));
}
