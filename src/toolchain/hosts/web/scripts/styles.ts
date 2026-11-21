// Prints the shared renderer's stylesheet, Codicons included, for the page to carry.
import { codiconStyle } from "../../../vscode-extension/client/src/osprey-ui/icons";
import { VIEW_STYLES, WORKSPACE_STYLES } from "../../../vscode-extension/client/src/osprey-ui/styles";

process.stdout.write(codiconStyle() + WORKSPACE_STYLES + VIEW_STYLES);
