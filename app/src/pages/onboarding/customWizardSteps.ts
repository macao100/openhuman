import type { CustomStepKey } from './OnboardingContext';

/** Ordered list of custom-wizard steps. Index drives the step counter UI and
 *  the back/continue navigation. `search` and `memory` are commented out for
 *  now — their pages still exist and route in case we want to re-enable. */
export const CUSTOM_WIZARD_STEPS: CustomStepKey[] = [
  'inference',
  'voice',
  'oauth',
  'search',
  'embeddings',
  // 'memory',
];

export const CUSTOM_WIZARD_ROUTES: Record<CustomStepKey, string> = {
  inference: '/onboarding/custom/inference',
  voice: '/onboarding/custom/voice',
  oauth: '/onboarding/custom/oauth',
  search: '/onboarding/custom/search',
  embeddings: '/onboarding/custom/embeddings',
  memory: '/onboarding/custom/memory',
};

