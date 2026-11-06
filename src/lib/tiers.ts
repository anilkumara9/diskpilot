import { CheckCircle2, Eye, Lock, type LucideIcon } from "lucide-react";
import type { Confidence, Tier } from "./types";

export interface TierMeta {
  id: Tier;
  label: string;
  shortLabel: string;
  textClass: string;
  bgSoftClass: string;
  icon: LucideIcon;
  description: string;
}

export const TIER_META: Record<Tier, TierMeta> = {
  likelySafe: {
    id: "likelySafe",
    label: "Likely safe",
    shortLabel: "Likely safe",
    textClass: "text-tier-safe",
    bgSoftClass: "bg-tier-safe/12",
    icon: CheckCircle2,
    description: "Regenerable caches, logs, and build output. Safe to clean in almost all cases.",
  },
  reviewFirst: {
    id: "reviewFirst",
    label: "Review first",
    shortLabel: "Review first",
    textClass: "text-tier-review",
    bgSoftClass: "bg-tier-review/12",
    icon: Eye,
    description: "Probably fine to remove, but worth a look — it may hold work you still want.",
  },
  keep: {
    id: "keep",
    label: "Keep",
    shortLabel: "Keep",
    textClass: "text-tier-keep",
    bgSoftClass: "bg-tier-keep/12",
    icon: Lock,
    description: "Personal data, settings, or history. DiskPilot reports these but never recommends removing them.",
  },
};

export const TIER_ORDER: Tier[] = ["likelySafe", "reviewFirst", "keep"];

export function tierMeta(tier: Tier): TierMeta {
  return TIER_META[tier];
}

export const CONFIDENCE_LABEL: Record<Confidence, string> = {
  high: "High confidence",
  medium: "Medium confidence",
  low: "Low confidence",
};
