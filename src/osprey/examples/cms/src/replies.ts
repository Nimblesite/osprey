// osprey: namespace=cms
// osprey: module=Replies

/** The JSON encoders and reply shapes of the API; JSON stays at this edge. [CMS-API] */
import { type Response } from "express";
import { type Commit, type Saved } from "./git.ts";
import { type Entry } from "./tree.ts";

/** `{"error": message}` with `status`. */
export function problem(res: Response, status: int, message: string) {
  return res.status(status).json(JsonObject([JsonMember("error", JsonString(message))]));
}

/** A failed git or file step: the server's fault, reported with its reason. */
export function failed(res: Response, message: string) {
  return problem(res, 500, message);
}

/** A commit reply: `{"commit", "path", "push"}`. [CMS-API-SAVE] */
export function savedJson(path: string, saved: Saved) {
  return JsonObject([JsonMember("commit", JsonString(saved.commit)), JsonMember("path", JsonString(path)), JsonMember("push", JsonString(saved.push))]);
}

function entryJson(entry: Entry) {
  return JsonObject([JsonMember("path", JsonString(entry.path)), JsonMember("kind", JsonString(entry.category))]);
}

/** The tree reply: `{"roots", "files"}`. [CMS-API-TREE] */
export function treeJson(roots: List<string>, entries: List<Entry>) {
  return JsonObject([JsonMember("roots", JsonArray(roots.map((root) => JsonString(root)))), JsonMember("files", JsonArray(entries.map(entryJson)))]);
}

function commitJson(commit: Commit) {
  return JsonObject([JsonMember("sha", JsonString(commit.sha)), JsonMember("subject", JsonString(commit.subject)), JsonMember("date", JsonString(commit.date))]);
}

/** The history reply: `{"commits"}`, newest first. */
export function historyJson(commits: List<Commit>) {
  return JsonObject([JsonMember("commits", JsonArray(commits.map(commitJson)))]);
}

/** A part that is not the last: `{"received": part}`. [CMS-API-UPLOAD] */
export function receivedJson(part: int) {
  return JsonObject([JsonMember("received", JsonInt(part))]);
}
