import { describe, expect, it } from "vitest";
import type {
  AssistantReply,
  ChatMessage,
  DockerActionRequest,
  DockerActionPreview,
  DockerActionResult,
  EmptyRecycleBinProgress,
  EnvironmentInfo,
  RecycleBinItem,
  ScanProgress,
} from "./types";

describe("shell contract shapes", () => {
  it("docker action request carries kind plus per-kind fields only", () => {
    const container: DockerActionRequest = {
      kind: "removeStoppedContainer",
      id: "abc123",
      name: "my-container",
    };
    const image: DockerActionRequest = {
      kind: "removeUnusedImage",
      id: "sha256:def",
      reference: "nginx:latest",
    };
    const cache: DockerActionRequest = { kind: "pruneBuildCache" };
    expect(Object.keys(container).sort()).toEqual(["id", "kind", "name"]);
    expect(Object.keys(image).sort()).toEqual(["id", "kind", "reference"]);
    expect(cache).toEqual({ kind: "pruneBuildCache" });
  });

  it("docker preview and result are command strings, not invented fields", () => {
    const preview: DockerActionPreview = { command: "docker rm abc123", description: "Remove it" };
    const result: DockerActionResult = { command: "docker rm abc123", output: "abc123" };
    expect(preview).toEqual({ command: "docker rm abc123", description: "Remove it" });
    expect(result.output).toBe("abc123");
  });

  it("recycle bin item and progress match the shell payloads", () => {
    const item: RecycleBinItem = {
      id: "C:\\$Recycle.Bin\\S-1\\$RABC.txt",
      name: "file.txt",
      originalPath: "C:\\Users\\me\\file.txt",
      bytes: 42,
      deletedAtMs: 1700000000000,
    };
    const progress: EmptyRecycleBinProgress = {
      completed: 1,
      total: 3,
      currentName: "file.txt",
      freedBytes: 42,
    };
    expect(item.originalPath).toContain("file.txt");
    expect(progress.currentName).toBe("file.txt");
  });

  it("environment, scan progress and assistant shapes match the shell", () => {
    const env: EnvironmentInfo = { os: "windows", arch: "x86_64", homeDir: "C:\\Users\\me", appVersion: "0.1.0" };
    const scan: ScanProgress = { currentPath: "C:\\Users\\me\\Docs", filesSeen: 10, bytesSeen: 2048 };
    const history: ChatMessage[] = [{ role: "user", content: "hi" }];
    const reply: AssistantReply = { content: "hello", highlightedRecommendationIds: ["r1"] };
    expect(env.os).toBe("windows");
    expect(scan.filesSeen).toBe(10);
    expect(history[0].role).toBe("user");
    expect(reply.highlightedRecommendationIds).toEqual(["r1"]);
  });
});
