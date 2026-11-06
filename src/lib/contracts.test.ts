import { describe, expect, it } from "vitest";
import type {
  AssistantReply,
  CandidateGroup,
  ChatMessage,
  DockerActionRequest,
  DockerActionPreview,
  DockerActionResult,
  EmptyRecycleBinProgress,
  EnvironmentInfo,
  ProvenanceGroup,
  RecentFile,
  RecycleBinItem,
  ScanComparison,
  ScanProgress,
  SearchResponse,
  SimilarGroup,
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

  it("scan comparison carries signed deltas and per-category rows", () => {
    const comparison: ScanComparison = {
      previousScannedAtMs: 1700000000000,
      totalBytesDelta: -1_200_000_000,
      fileCountDelta: -56,
      categories: [
        { category: "cache", currentBytes: 800, previousBytes: 2_000, deltaBytes: -1_200 },
        { category: "userData", currentBytes: 5_000, previousBytes: 5_000, deltaBytes: 0 },
      ],
    };
    expect(Object.keys(comparison).sort()).toEqual([
      "categories",
      "fileCountDelta",
      "previousScannedAtMs",
      "totalBytesDelta",
    ]);
    expect(Object.keys(comparison.categories[0]).sort()).toEqual([
      "category",
      "currentBytes",
      "deltaBytes",
      "previousBytes",
    ]);
    expect(comparison.totalBytesDelta).toBeLessThan(0);
  });

  it("downloads provenance shapes match the core provenance module", () => {
    const recent: RecentFile = {
      path: "C:\\Users\\me\\Downloads\\deck.pdf",
      name: "deck.pdf",
      bytes: 2048,
      modifiedMs: 1700000000000,
      source: {
        zoneId: 3,
        hostUrl: "https://mail.google.com/attachment",
        referrerUrl: null,
        label: "Gmail",
      },
    };
    expect(Object.keys(recent).sort()).toEqual(["bytes", "modifiedMs", "name", "path", "source"]);
    const group: ProvenanceGroup = {
      label: "Gmail",
      host: "mail.google.com",
      files: [{ path: recent.path, bytes: 2048, modifiedMs: 1700000000000 }],
      totalBytes: 2048,
    };
    expect(group.files[0].modifiedMs).toBe(1700000000000);
    const noSource: RecentFile = { ...recent, source: null };
    expect(noSource.source).toBeNull();
  });

  it("search response echoes filters in the core SearchFilters shape", () => {
    const response: SearchResponse = {
      chips: [
        { kind: "kind", label: "PDFs" },
        { kind: "date", label: "Last week" },
        { kind: "size", label: "> 5 MB" },
      ],
      filters: {
        kinds: ["pdf"],
        minBytes: 5_000_000,
        maxBytes: null,
        modifiedAfterMs: 1700000000000,
        modifiedBeforeMs: null,
        source: null,
        terms: [],
      },
      hits: [
        {
          path: "C:\\Users\\me\\deck.pdf",
          name: "deck.pdf",
          bytes: 6_100_000,
          category: "userData",
          modifiedMs: 1700000000000,
        },
      ],
      totalMatches: 1,
    };
    expect(Object.keys(response.filters).sort()).toEqual([
      "kinds",
      "maxBytes",
      "minBytes",
      "modifiedAfterMs",
      "modifiedBeforeMs",
      "source",
      "terms",
    ]);
    expect(response.chips.map((c) => c.kind)).toEqual(["kind", "date", "size"]);
  });

  it("similar image groups are largest-first with a wasted total", () => {
    const group: SimilarGroup = {
      files: [
        { path: "C:\\Users\\me\\a.jpg", bytes: 3000, width: 4000, height: 3000 },
        { path: "C:\\Users\\me\\b.jpg", bytes: 1200, width: 2000, height: 1500 },
      ],
      wastedBytes: 1200,
    };
    expect(group.files[0].bytes).toBeGreaterThan(group.files[1].bytes);
    expect(group.wastedBytes).toBe(1200);
  });

  it("cleanup card groups carry candidate files with core-assigned ids", () => {
    const group: CandidateGroup = {
      card: "old-installers",
      title: "Old installers",
      files: [
        {
          id: "0123456789abcdef",
          path: "C:\\Users\\me\\Downloads\\driver.msi",
          name: "driver.msi",
          bytes: 4096,
          modifiedMs: 1700000000000,
        },
      ],
      totalBytes: 4096,
    };
    expect(Object.keys(group).sort()).toEqual(["card", "files", "title", "totalBytes"]);
    expect(Object.keys(group.files[0]).sort()).toEqual([
      "bytes",
      "id",
      "modifiedMs",
      "name",
      "path",
    ]);
    expect(group.files[0].id).toHaveLength(16);
    expect(group.totalBytes).toBe(group.files[0].bytes);
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
