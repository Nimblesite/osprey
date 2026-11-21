// The web host's entry: the bridge first, so the shared webview client finds it when it loads.
import "./bridge";
import "../../../vscode-extension/client/src/osprey-ui/webview/main";
