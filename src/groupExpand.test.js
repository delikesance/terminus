/**
 * Selftest for group expand/clear search behavior
 * Tests the expand ancestors logic and clear query coherence
 */

import { computeExpandedGroups, shouldExpandGroup } from "./groupExpand.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];

  // Test data: nested group hierarchy
  //   Production (root)
  //     ├─ East (child)
  //     │   └─ East-DB (grandchild)
  //     └─ West (child)
  const groups = [
    { id: "prod", name: "Production", parent_id: null },
    { id: "east", name: "East", parent_id: "prod" },
    { id: "east-db", name: "East-DB", parent_id: "east" },
    { id: "west", name: "West", parent_id: "prod" },
  ];

  const hosts = [
    { id: "h1", name: "db-east-1", group_id: "east-db" },
    { id: "h2", name: "web-east-1", group_id: "east" },
    { id: "h3", name: "web-west-1", group_id: "west" },
  ];

  // Test 1: When a nested host matches, ancestors should be expanded
  {
    const matchingHosts = hosts.filter((h) => h.name.includes("db-east"));
    const matchingGroupIds = new Set();
    const expanded = computeExpandedGroups(groups, matchingGroupIds, matchingHosts);

    const hasEastDB = expanded.has("east-db");
    const hasEast = expanded.has("east");
    const hasProd = expanded.has("prod");
    const hasWest = expanded.has("west");

    checks.push(
      check(
        "expand_nested_host_match",
        hasEastDB && hasEast && hasProd && !hasWest,
        `Matched host in East-DB → expanded: east-db=${hasEastDB}, east=${hasEast}, prod=${hasProd}, west=${hasWest}`,
      ),
    );
  }

  // Test 2: When a nested group matches, its ancestors should be expanded
  {
    const matchingHosts = [];
    const matchingGroupIds = new Set(["east-db"]);
    const expanded = computeExpandedGroups(groups, matchingGroupIds, matchingHosts);

    const hasEast = expanded.has("east");
    const hasProd = expanded.has("prod");
    const hasEastDB = expanded.has("east-db");

    checks.push(
      check(
        "expand_nested_group_match",
        hasEast && hasProd && !hasEastDB,
        `Matched group East-DB → expanded ancestors: east=${hasEast}, prod=${hasProd}, east-db=${hasEastDB}`,
      ),
    );
  }

  // Test 3: When search query is cleared, only persistent expansion should remain
  {
    const persistentExpanded = new Set(["prod"]); // User had previously expanded "prod"
    const matchingGroupIds = new Set(["east-db"]);
    const matchingHosts = [];
    const searchExpanded = computeExpandedGroups(groups, matchingGroupIds, matchingHosts);

    // During search: prod, east, and east-db should be considered
    const duringSearch = {
      prod: shouldExpandGroup("prod", persistentExpanded, searchExpanded, matchingGroupIds, true),
      east: shouldExpandGroup("east", persistentExpanded, searchExpanded, matchingGroupIds, true),
      eastDB: shouldExpandGroup("east-db", persistentExpanded, searchExpanded, matchingGroupIds, true),
      west: shouldExpandGroup("west", persistentExpanded, searchExpanded, matchingGroupIds, true),
    };

    // After clearing search: only prod should be expanded
    const afterClear = {
      prod: shouldExpandGroup("prod", persistentExpanded, searchExpanded, matchingGroupIds, false),
      east: shouldExpandGroup("east", persistentExpanded, searchExpanded, matchingGroupIds, false),
      eastDB: shouldExpandGroup("east-db", persistentExpanded, searchExpanded, matchingGroupIds, false),
      west: shouldExpandGroup("west", persistentExpanded, searchExpanded, matchingGroupIds, false),
    };

    const searchOk = duringSearch.prod && duringSearch.east && duringSearch.eastDB && !duringSearch.west;
    const clearOk = afterClear.prod && !afterClear.east && !afterClear.eastDB && !afterClear.west;

    checks.push(
      check(
        "clear_search_coherence",
        searchOk && clearOk,
        `During search: prod=${duringSearch.prod}, east=${duringSearch.east}, east-db=${duringSearch.eastDB}, west=${duringSearch.west}; After clear: prod=${afterClear.prod}, east=${afterClear.east}, east-db=${afterClear.eastDB}, west=${afterClear.west}`,
      ),
    );
  }

  // Test 4: Multiple matching hosts in different branches
  {
    const matchingHosts = hosts.filter((h) => h.name.includes("web"));
    const matchingGroupIds = new Set();
    const expanded = computeExpandedGroups(groups, matchingGroupIds, matchingHosts);

    const hasEast = expanded.has("east");
    const hasWest = expanded.has("west");
    const hasProd = expanded.has("prod");

    checks.push(
      check(
        "expand_multiple_branches",
        hasEast && hasWest && hasProd,
        `Matched hosts in East and West → expanded: east=${hasEast}, west=${hasWest}, prod=${hasProd}`,
      ),
    );
  }

  // Test 5: Empty search should not force any expansion
  {
    const persistentExpanded = new Set();
    const searchExpanded = new Set();
    const matchingGroupIds = new Set();

    const anyExpanded =
      shouldExpandGroup("prod", persistentExpanded, searchExpanded, matchingGroupIds, false) ||
      shouldExpandGroup("east", persistentExpanded, searchExpanded, matchingGroupIds, false) ||
      shouldExpandGroup("west", persistentExpanded, searchExpanded, matchingGroupIds, false);

    checks.push(
      check(
        "empty_search_no_expansion",
        !anyExpanded,
        `With no search and no persistent state, no groups should be expanded: ${anyExpanded}`,
      ),
    );
  }

  // Test 6: Search matches root group - only that group should be marked for expansion
  {
    const matchingHosts = [];
    const matchingGroupIds = new Set(["prod"]);
    const expanded = computeExpandedGroups(groups, matchingGroupIds, matchingHosts);

    const hasProd = expanded.has("prod");
    const hasAnyChild = expanded.has("east") || expanded.has("west") || expanded.has("east-db");

    checks.push(
      check(
        "root_group_match_no_ancestors",
        !hasProd && !hasAnyChild,
        `Root group match has no ancestors to expand: prod=${hasProd}, any-child=${hasAnyChild}`,
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
