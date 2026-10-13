// The company's pages of phase 3 (Trello "Tauri 3/6") by their tab (`company::TABS`): the
// planning, the depot, the concessions, the fleet map, the research, the workshop and the
// contracts. The company's frame (`company.js`) draws the one open through `MorePage`.

import { html } from './kit.js';
import { MORE_TABS } from './more.js';
import { PlanningPage } from './planning.js';
import { DepotPage } from './depot.js';
import { ConcessionsPage } from './concessions.js';
import { FleetMapPage } from './fleetmap.js';
import { ResearchPage } from './research.js';
import { WorkshopPage } from './workshop.js';
import { ContractsPage } from './contracts.js';

export { MORE_TABS };

const PAGES = { 5: PlanningPage, 7: DepotPage, 8: ConcessionsPage, 9: FleetMapPage, 10: ResearchPage, 11: WorkshopPage, 12: ContractsPage };

/** The page of tab `tab`. */
export function MorePage({ tab }) {
  const Page = PAGES[tab];
  return Page ? html`<${Page} />` : null;
}
