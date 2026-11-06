import type { Classification } from "@/lib/types";
import { CONFIDENCE_LABEL } from "@/lib/tiers";
import { CategoryBadge } from "./CategoryBadge";

export function ClassificationCard({ classification }: { classification: Classification }) {
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <CategoryBadge category={classification.category} />
        <span className="text-[11px] text-muted">{CONFIDENCE_LABEL[classification.confidence]}</span>
      </div>
      <dl className="space-y-1.5 text-[12px] leading-4">
        <div className="flex gap-2">
          <dt className="w-[86px] shrink-0 text-muted">Rule</dt>
          <dd className="min-w-0">
            <span className="font-medium text-foreground">{classification.label}</span>{" "}
            <span className="font-mono text-[11px] text-subtle">({classification.ruleId})</span>
          </dd>
        </div>
        <div className="flex gap-2">
          <dt className="w-[86px] shrink-0 text-muted">Why</dt>
          <dd className="min-w-0 text-foreground">{classification.explanation}</dd>
        </div>
        {classification.inheritedFrom ? (
          <div className="flex gap-2">
            <dt className="w-[86px] shrink-0 text-muted">Inherited from</dt>
            <dd className="mono-path min-w-0 pt-0.5 text-foreground">{classification.inheritedFrom}</dd>
          </div>
        ) : null}
      </dl>
    </div>
  );
}
