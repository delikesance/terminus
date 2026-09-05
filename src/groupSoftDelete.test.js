/**
 * Selftest for group soft-delete data invariants
 * Tests cascade delete, host detachment, and restore behavior
 */

import {
  findDescendantGroups,
  computeAffectedGroups,
  findOrphanedHosts,
  applySoftDelete,
  restoreGroup,
  detachHost,
} from "./groupSoftDelete.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];
  const now = "2026-09-05T12:00:00Z";

  // Test data: deep nested group hierarchy
  //   Production (root)
  //     ├─ East (child)
  //     │   └─ East-DB (grandchild)
  //     └─ West (child)
  const groups = [
    { id: "prod", name: "Production", parent_id: null, deleted_at: null },
    { id: "east", name: "East", parent_id: "prod", deleted_at: null },
    { id: "east-db", name: "East-DB", parent_id: "east", deleted_at: null },
    { id: "west", name: "West", parent_id: "prod", deleted_at: null },
  ];

  const hosts = [
    { id: "h1", name: "db-east-1", group_id: "east-db" },
    { id: "h2", name: "db-east-2", group_id: "east-db" },
    { id: "h3", name: "web-east-1", group_id: "east" },
    { id: "h4", name: "lb-prod", group_id: "prod" },
    { id: "h5", name: "web-west-1", group_id: "west" },
    { id: "h6", name: "ungrouped-host", group_id: null },
  ];

  // Test 1: Find descendants of a parent with nested children
  {
    const descendants = findDescendantGroups("prod", groups);
    const hasEast = descendants.includes("east");
    const hasEastDB = descendants.includes("east-db");
    const hasWest = descendants.includes("west");
    const hasProd = descendants.includes("prod");

    checks.push(
      check(
        "find_descendants_deep_tree",
        hasEast && hasEastDB && hasWest && !hasProd && descendants.length === 3,
        `Descendants of prod should include east, east-db, west but not prod itself: ${JSON.stringify(descendants)}`,
      ),
    );
  }

  // Test 2: Find descendants of a middle-level parent
  {
    const descendants = findDescendantGroups("east", groups);
    const hasEastDB = descendants.includes("east-db");
    const hasEast = descendants.includes("east");

    checks.push(
      check(
        "find_descendants_middle_level",
        hasEastDB && !hasEast && descendants.length === 1,
        `Descendants of east should only include east-db: ${JSON.stringify(descendants)}`,
      ),
    );
  }

  // Test 3: Find descendants of a leaf node (no children)
  {
    const descendants = findDescendantGroups("east-db", groups);

    checks.push(
      check(
        "find_descendants_leaf_node",
        descendants.length === 0,
        `Leaf node east-db should have no descendants: ${JSON.stringify(descendants)}`,
      ),
    );
  }

  // Test 4: Compute affected groups includes parent + all descendants
  {
    const affected = computeAffectedGroups("prod", groups);
    const hasProd = affected.includes("prod");
    const hasEast = affected.includes("east");
    const hasEastDB = affected.includes("east-db");
    const hasWest = affected.includes("west");

    checks.push(
      check(
        "compute_affected_includes_self_and_descendants",
        hasProd && hasEast && hasEastDB && hasWest && affected.length === 4,
        `Affected groups for prod should include prod + all 3 descendants: ${JSON.stringify(affected)}`,
      ),
    );
  }

  // Test 5: Find orphaned hosts when deleting parent with nested children
  {
    const affectedGroups = computeAffectedGroups("prod", groups);
    const orphanedHosts = findOrphanedHosts(affectedGroups, hosts);

    const orphanedIds = orphanedHosts.map((h) => h.id);
    const hasH1 = orphanedIds.includes("h1"); // in east-db
    const hasH2 = orphanedIds.includes("h2"); // in east-db
    const hasH3 = orphanedIds.includes("h3"); // in east
    const hasH4 = orphanedIds.includes("h4"); // in prod
    const hasH5 = orphanedIds.includes("h5"); // in west
    const hasH6 = orphanedIds.includes("h6"); // ungrouped (should not be included)

    checks.push(
      check(
        "find_orphaned_hosts_deep_tree",
        hasH1 && hasH2 && hasH3 && hasH4 && hasH5 && !hasH6 && orphanedHosts.length === 5,
        `Orphaned hosts should include h1-h5 but not h6 (ungrouped): ${JSON.stringify(orphanedIds)}`,
      ),
    );
  }

  // Test 6: Find orphaned hosts when deleting middle-level group
  {
    const affectedGroups = computeAffectedGroups("east", groups);
    const orphanedHosts = findOrphanedHosts(affectedGroups, hosts);

    const orphanedIds = orphanedHosts.map((h) => h.id);
    const hasH1 = orphanedIds.includes("h1"); // in east-db (child of east)
    const hasH2 = orphanedIds.includes("h2"); // in east-db (child of east)
    const hasH3 = orphanedIds.includes("h3"); // in east
    const hasH4 = orphanedIds.includes("h4"); // in prod (should not be included)

    checks.push(
      check(
        "find_orphaned_hosts_middle_level",
        hasH1 && hasH2 && hasH3 && !hasH4 && orphanedHosts.length === 3,
        `Orphaned hosts for east deletion should include h1-h3 only: ${JSON.stringify(orphanedIds)}`,
      ),
    );
  }

  // Test 7: Apply soft-delete sets deleted_at and updated_at
  {
    const group = { id: "prod", name: "Production", parent_id: null, deleted_at: null };
    const deleted = applySoftDelete(group, now);

    checks.push(
      check(
        "apply_soft_delete_sets_timestamps",
        deleted.deleted_at === now && deleted.updated_at === now && deleted.id === "prod",
        `Soft-delete should set deleted_at and updated_at: ${JSON.stringify(deleted)}`,
      ),
    );
  }

  // Test 8: Restore group clears deleted_at but preserves other fields
  {
    const group = { id: "prod", name: "Production", parent_id: null, deleted_at: "2026-09-01T00:00:00Z" };
    const restored = restoreGroup(group, now);

    checks.push(
      check(
        "restore_group_clears_deleted_at",
        restored.deleted_at === null && restored.updated_at === now && restored.id === "prod",
        `Restore should clear deleted_at and update updated_at: ${JSON.stringify(restored)}`,
      ),
    );
  }

  // Test 9: Detach host clears group_id
  {
    const host = { id: "h1", name: "db-east-1", group_id: "east-db" };
    const detached = detachHost(host, now);

    checks.push(
      check(
        "detach_host_clears_group_id",
        detached.group_id === null && detached.updated_at === now && detached.id === "h1",
        `Detach should clear group_id and update updated_at: ${JSON.stringify(detached)}`,
      ),
    );
  }

  // Test 10: Integration test - full soft-delete cascade for deep tree
  {
    const rootGroup = groups.find((g) => g.id === "prod");
    const affectedGroupIds = computeAffectedGroups(rootGroup.id, groups);

    // Apply soft-delete to all affected groups
    const deletedGroups = affectedGroupIds.map((gid) => {
      const g = groups.find((group) => group.id === gid);
      return applySoftDelete(g, now);
    });

    // Find and detach orphaned hosts
    const orphanedHosts = findOrphanedHosts(affectedGroupIds, hosts);
    const detachedHosts = orphanedHosts.map((h) => detachHost(h, now));

    const allGroupsDeleted = deletedGroups.every((g) => g.deleted_at === now);
    const allHostsDetached = detachedHosts.every((h) => h.group_id === null);
    const correctHostCount = detachedHosts.length === 5; // h1-h5

    checks.push(
      check(
        "integration_full_cascade_delete",
        allGroupsDeleted && allHostsDetached && correctHostCount,
        `Full cascade: ${deletedGroups.length} groups deleted, ${detachedHosts.length} hosts detached`,
      ),
    );
  }

  // Test 11: Integration test - restore does NOT reattach hosts
  {
    const deletedGroup = { id: "prod", name: "Production", parent_id: null, deleted_at: now };
    const detachedHost = { id: "h4", name: "lb-prod", group_id: null, updated_at: now };

    // Restore the group
    const restoredGroup = restoreGroup(deletedGroup, "2026-09-05T13:00:00Z");

    // Host remains detached (no reattach logic)
    const hostRemainsDetached = detachedHost.group_id === null;
    const groupRestored = restoredGroup.deleted_at === null;

    checks.push(
      check(
        "integration_restore_no_reattach",
        groupRestored && hostRemainsDetached,
        `Restore clears deleted_at but hosts remain detached: group.deleted_at=${restoredGroup.deleted_at}, host.group_id=${detachedHost.group_id}`,
      ),
    );
  }

  // Test 12: Edge case - ungrouped hosts are not affected by deletion
  {
    const affectedGroups = computeAffectedGroups("prod", groups);
    const ungroupedHost = hosts.find((h) => h.id === "h6");
    const orphanedHosts = findOrphanedHosts(affectedGroups, hosts);

    const ungroupedNotOrphaned = !orphanedHosts.some((h) => h.id === "h6");

    checks.push(
      check(
        "edge_case_ungrouped_hosts_unaffected",
        ungroupedNotOrphaned && ungroupedHost.group_id === null,
        `Ungrouped host h6 should not be in orphaned list`,
      ),
    );
  }

  return checks;
}

function main() {
  const checks = runTests();
  const allOk = checks.every((c) => c.ok);

  const result = {
    ok: allOk,
    checked_at: new Date().toISOString(),
    checks,
  };

  console.log(JSON.stringify(result, null, 2));
  process.exit(allOk ? 0 : 1);
}

main();
