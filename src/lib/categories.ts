import {
  AppWindow,
  Binary,
  Box,
  Code2,
  Database,
  Download,
  FileQuestion,
  FolderCog,
  HardDrive,
  ScrollText,
  Sparkles,
  type LucideIcon,
} from "lucide-react";
import type { Category } from "./types";

export interface CategoryMeta {
  id: Category;
  label: string;
  /** CSS variable holding the category hue (switches with theme). */
  colorVar: string;
  /** Tailwind text-color utility bound to the theme token. */
  textClass: string;
  /** Tailwind bg utility bound to the theme token. */
  bgClass: string;
  icon: LucideIcon;
  description: string;
}

export const CATEGORY_META: Record<Category, CategoryMeta> = {
  applicationData: {
    id: "applicationData",
    label: "Application data",
    colorVar: "var(--cat-application-data)",
    textClass: "text-cat-application-data",
    bgClass: "bg-cat-application-data",
    icon: AppWindow,
    description: "Data apps keep in AppData — settings, databases, and working files.",
  },
  buildArtifacts: {
    id: "buildArtifacts",
    label: "Build artifacts",
    colorVar: "var(--cat-build-artifacts)",
    textClass: "text-cat-build-artifacts",
    bgClass: "bg-cat-build-artifacts",
    icon: Sparkles,
    description: "Compiled output such as node_modules, target, and bin/obj folders. Regenerable from source.",
  },
  dockerStorage: {
    id: "dockerStorage",
    label: "Docker storage",
    colorVar: "var(--cat-docker-storage)",
    textClass: "text-cat-docker-storage",
    bgClass: "bg-cat-docker-storage",
    icon: Box,
    description: "Docker images, containers, volumes, and the WSL virtual disk that holds them.",
  },
  cache: {
    id: "cache",
    label: "Cache",
    colorVar: "var(--cat-cache)",
    textClass: "text-cat-cache",
    bgClass: "bg-cat-cache",
    icon: Database,
    description: "Temporary caches that tools rebuild on demand, such as npm, pip, and browser caches.",
  },
  userData: {
    id: "userData",
    label: "User data",
    colorVar: "var(--cat-user-data)",
    textClass: "text-cat-user-data",
    bgClass: "bg-cat-user-data",
    icon: HardDrive,
    description: "Your documents, photos, projects, and other personal files.",
  },
  downloads: {
    id: "downloads",
    label: "Downloads",
    colorVar: "var(--cat-downloads)",
    textClass: "text-cat-downloads",
    bgClass: "bg-cat-downloads",
    icon: Download,
    description: "Files in the Downloads folder.",
  },
  developerTools: {
    id: "developerTools",
    label: "Developer tools",
    colorVar: "var(--cat-developer-tools)",
    textClass: "text-cat-developer-tools",
    bgClass: "bg-cat-developer-tools",
    icon: Code2,
    description: "Editors, AI coding tools, SDKs, and toolchains installed for development.",
  },
  logs: {
    id: "logs",
    label: "Logs",
    colorVar: "var(--cat-logs)",
    textClass: "text-cat-logs",
    bgClass: "bg-cat-logs",
    icon: ScrollText,
    description: "Log files written by apps and tools. Useful for troubleshooting, rarely worth keeping forever.",
  },
  binaryData: {
    id: "binaryData",
    label: "Blob / binary data",
    colorVar: "var(--cat-binary-data)",
    textClass: "text-cat-binary-data",
    bgClass: "bg-cat-binary-data",
    icon: Binary,
    description: "Large binary blobs such as model weights, disk images, and archives.",
  },
  systemFiles: {
    id: "systemFiles",
    label: "System files",
    colorVar: "var(--cat-system-files)",
    textClass: "text-cat-system-files",
    bgClass: "bg-cat-system-files",
    icon: FolderCog,
    description: "Windows system files and protected locations. Reported for context, never cleaned by DiskPilot.",
  },
  unknown: {
    id: "unknown",
    label: "Unknown",
    colorVar: "var(--cat-unknown)",
    textClass: "text-cat-unknown",
    bgClass: "bg-cat-unknown",
    icon: FileQuestion,
    description: "Files that did not match any classification rule.",
  },
};

export const CATEGORY_ORDER: Category[] = [
  "userData",
  "applicationData",
  "buildArtifacts",
  "cache",
  "developerTools",
  "dockerStorage",
  "downloads",
  "binaryData",
  "logs",
  "systemFiles",
  "unknown",
];

export function categoryMeta(category: Category): CategoryMeta {
  return CATEGORY_META[category] ?? CATEGORY_META.unknown;
}
