/**
 * Group soft-delete logic for cascade delete and host detachment
 * Extracted for testing deletion data invariants
 */

/**
 * Find all descendant group IDs recursively
 * @param {string} parentId Starting parent group ID
 * @param {Array<{id: string, parent_id?: string | null}>} groups All groups
 * @returns {string[]} Array of all descendant group IDs
 */
export function findDescendantGroups(parentId, groups) {
  const children = groups.filter((g) => g.parent_id === parentId).map((g) => g.id);
  const descendants = [...children];
  for (const childId of children) {
    descendants.push(...findDescendantGroups(childId, groups));
  }
  return descendants;
}

/**
 * Compute which groups should be soft-deleted (marked with deleted_at)
 * @param {string} groupId The group being deleted
 * @param {Array<{id: string, parent_id?: string | null}>} groups All groups
 * @returns {string[]} Array of group IDs to soft-delete (parent + all descendants)
 */
export function computeAffectedGroups(groupId, groups) {
  const descendants = findDescendantGroups(groupId, groups);
  return [groupId, ...descendants];
}

/**
 * Find all hosts that should be detached (group_id cleared) when a group is soft-deleted
 * @param {string[]} affectedGroupIds Group IDs being soft-deleted
 * @param {Array<{id: string, group_id?: string | null}>} hosts All hosts
 * @returns {Array<{id: string, group_id?: string | null}>} Hosts that need group_id cleared
 */
export function findOrphanedHosts(affectedGroupIds, hosts) {
  return hosts.filter((h) => h.group_id && affectedGroupIds.includes(h.group_id));
}

/**
 * Apply soft-delete to a group (set deleted_at timestamp)
 * @param {Object} group Group object
 * @param {string} timestamp ISO timestamp for deleted_at and updated_at
 * @returns {Object} Updated group object
 */
export function applySoftDelete(group, timestamp) {
  return {
    ...group,
    deleted_at: timestamp,
    updated_at: timestamp,
  };
}

/**
 * Restore a soft-deleted group (clear deleted_at)
 * This does NOT reattach hosts - they remain in Ungrouped
 * @param {Object} group Group object
 * @param {string} timestamp ISO timestamp for updated_at
 * @returns {Object} Updated group object with deleted_at cleared
 */
export function restoreGroup(group, timestamp) {
  return {
    ...group,
    deleted_at: null,
    updated_at: timestamp,
  };
}

/**
 * Detach a host from its group (clear group_id)
 * @param {Object} host Host object
 * @param {string} timestamp ISO timestamp for updated_at
 * @returns {Object} Updated host object with group_id cleared
 */
export function detachHost(host, timestamp) {
  return {
    ...host,
    group_id: null,
    updated_at: timestamp,
  };
}
