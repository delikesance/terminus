# Example nixpkgs callPackage entry (for upstreaming).
# Prefer attribute `terminus`; rename to `terminus-ssh` if taken.
#
# In nixpkgs: pkgs/by-name/te/terminus/package.nix (or similar) should track
# this repo's nix/package.nix. Until merged, install via FlakeHub / Cachix.
{
  lib,
  callPackage,
  fetchFromGitHub,
}:

callPackage ./package.nix {
  src = fetchFromGitHub {
    owner = "delikesance";
    repo = "terminus";
    rev = "REPLACE_WITH_TAG_OR_COMMIT";
    hash = "sha256-REPLACE";
  };
  version = "REPLACE_WITH_VERSION";
}
