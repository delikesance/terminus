/**
 * Group soft-delete logic for cascade delete and host detachment
 * Extracted for testing deletion data invariants
 */

type Group = {
  id: string;
  name: string;
  parent_id?: string | null;
  deleted_at?: string | null;
  created_at?: string;
  updated_at?: string;
};

type Host = {
  id: string;
  name?: string;
  hostname?: string;
  port?: number;
  username?: string;
  group_id?: string | null;
  updated_at?: string;
  [key: string]: unknown;
};

/**
 * Find all descendant group IDs recursively
 * @param parentId Starting parent group ID
 * @param groups All groups
 * @returns Array of all descendant group IDs
 */
export function findDescendantGroups(parentId: string, groups: Group[]): string[] {
  const children = groups.filter((g) => g.parent_id === parentId).map((g) => g.id);
  const descendants = [...children];
  for (const childId of children) {
    descendants.push(...findDescendantGroups(childId, groups));
  }
  return descendants;
}

/**
 * Compute which groups should be soft-deleted (marked with deleted_at)
 * @param groupId The group being deleted
 * @param groups All groups
 * @returns Array of group IDs to soft-delete (parent + all descendants)
 */
export function computeAffectedGroups(groupId: string, groups: Group[]): string[] {
  const descendants = findDescendantGroups(groupId, groups);
  return [groupId, ...descendants];
}

/**
 * Find all hosts that should be detached (group_id cleared) when a group is soft-deleted
 * @param affectedGroupIds Group IDs being soft-deleted
 * @param hosts All hosts
 * @returns Hosts that need group_id cleared
 */
export function findOrphanedHosts(affectedGroupIds: string[], hosts: Host[]): Host[] {
  return hosts.filter((h) => h.group_id && affectedGroupIds.includes(h.group_id));
}

/**
 * Apply soft-delete to a group (set deleted_at timestamp)
 * @param group Group object
 * @param timestamp ISO timestamp for deleted_at and updated_at
 * @returns Updated group object
 */
export function applySoftDelete(group: Group, timestamp: string): Group {
  return {
    ...group,
    deleted_at: timestamp,
    updated_at: timestamp,
  };
}

/**
 * Restore a soft-deleted group (clear deleted_at)
 * This does NOT reattach hosts - they remain in Ungrouped
 * @param group Group object
 * @param timestamp ISO timestamp for updated_at
 * @returns Updated group object with deleted_at cleared
 */
export function restoreGroup(group: Group, timestamp: string): Group {
  return {
    ...group,
    deleted_at: null,
    updated_at: timestamp,
  };
}

/**
 * Detach a host from its group (clear group_id)
 * @param host Host object
 * @param timestamp ISO timestamp for updated_at
 * @returns Updated host object with group_id cleared
 */
export function detachHost(host: Host, timestamp: string): Host {
  return {
    ...host,
    group_id: null,
    updated_at: timestamp,
  };
}
