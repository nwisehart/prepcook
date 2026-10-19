import type { ExecutorConfig } from 'shared/types';
import { toPrettyCase } from '@/shared/lib/string';

// Claude model ids carry the family between a vendor prefix and a version
// suffix ("anthropic/claude-opus-4-1-20250805"); the family is what people
// call the model.
const CLAUDE_FAMILY = /(opus|sonnet|haiku)/i;

function modelName(modelId: string): string {
  const last = modelId.split('/').pop() || modelId;
  const family = last.match(CLAUDE_FAMILY)?.[1];
  if (!family) return last;
  return family.charAt(0).toUpperCase() + family.slice(1).toLowerCase();
}

/**
 * "Opus · high": the model a session runs and its reasoning effort. Falls
 * back to the preset, then to the agent, when no model override is set.
 */
export function formatModelLabel(
  config: ExecutorConfig | null | undefined
): string | null {
  if (!config) return null;
  const model = config.model_id
    ? modelName(config.model_id)
    : config.variant && config.variant.toUpperCase() !== 'DEFAULT'
      ? toPrettyCase(config.variant)
      : toPrettyCase(config.executor);
  return config.reasoning_id ? `${model} · ${config.reasoning_id}` : model;
}
