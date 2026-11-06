import { MarkerTag } from "./MarkerTag";
import { Tooltip } from "@/components/ui/tooltip";

/**
 * Neutral chip on a search hit that matched through text read
 * inside the file (OCR) rather than its name. Deliberately the
 * plain marker tag — never a tier tag, which is reserved for
 * cleanup risk. Used by Storage search results and Quick Search.
 */
export function TextMatchChip() {
  return (
    <Tooltip content="Matched text inside this file" side="bottom">
      <MarkerTag className="shrink-0">Text match</MarkerTag>
    </Tooltip>
  );
}
