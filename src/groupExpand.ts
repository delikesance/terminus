/**
 * Group expansion logic for search filtering
 * Extracted for testing expand/clear search behavior
 */

type Group = {
  id: string;
  name: string;
  parent_id?: string | null;
};

type Host = {
  id: string;
  name: string;
  group_id?: string | null;
};

/**
 * Compute which groups should be expanded based on search query and matches
 * @param groups All groups
 * @param matchingGroupIds IDs of groups that match the search query
 * @param matchingHosts Hosts that match the search query
 * @returns Set of group IDs that should be expanded to reveal matches
 */
export function computeExpandedGroups(
  groups: Group[],
  matchingGroupIds: Set<string>,
  matchingHosts: Host[],
): Set<string> {
  const expandedBySearch = new Set<string>();

  // Helper to expand all ancestors of a group
  const expandAncestors = (groupId: string) => {
    const group = groups.find((g) => g.id === groupId);
    if (group?.parent_id) {
      expandedBySearch.add(group.parent_id);
      expandAncestors(group.parent_id);
    }
  };

  // If a host matches, expand its group and all ancestor groups
  for (const host of matchingHosts) {
    if (host.group_id) {
      expandedBySearch.add(host.group_id);
      expandAncestors(host.group_id);
    }
  }

  // If a group matches, expand its ancestors
  for (const groupId of matchingGroupIds) {
    expandAncestors(groupId);
  }

  return expandedBySearch;
}

/**
 * Check if a group should be expanded based on persistent state and search-driven expansion
 * @param groupId Group ID to check
 * @param persistentExpanded User's persistent expansion choices (from localStorage)
 * @param searchExpanded Groups expanded due to search matches
 * @param matchingGroupIds Groups that match the search query
 * @param hasSearchQuery Whether a search query is active
 * @returns true if the group should be expanded
 */
export function shouldExpandGroup(
  groupId: string,
  persistentExpanded: Set<string>,
  searchExpanded: Set<string>,
  matchingGroupIds: Set<string>,
  hasSearchQuery: boolean,
): boolean {
  if (hasSearchQuery) {
    // During search: expand if user previously expanded OR if search requires it OR if the group itself matches
    return persistentExpanded.has(groupId) || searchExpanded.has(groupId) || matchingGroupIds.has(groupId);
  } else {
    // No search: only respect user's persistent choices
    return persistentExpanded.has(groupId);
  }
}
