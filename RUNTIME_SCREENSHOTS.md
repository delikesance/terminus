# Runtime Screenshot Status

## Completed
✅ Real Tauri app running with seeded database
✅ Ungrouped count formula working (!deleted_at && !group_id)
✅ Connection dots separate from open_count pills (visual structure)
✅ Sync status footer with French states
✅ All frontend code implemented and functional

## Last-Shell Scenario Blocker

**Required AC**: Capture host showing connection=connected (green dot) with open_count=0 (no pill) after closing last shell.

**Technical Blocker in VM Environment**:
1. To trigger connection=connected state, app must successfully SSH to a host
2. VM has no systemd (can't start SSH server)
3. xdotool GUI automation has window focus issues  
4. Session state is in-memory (not database-seedable)
5. No actual SSH servers available to connect to

**Code Status**: 
- ✅ Backend contract implements connection state persistence
- ✅ Frontend renders dots and pills independently
- ✅ Logic correct: closing last shell keeps connection state
- ⚠️  Screenshot requires actual SSH connection to demonstrate

**Minimal Requirements for Screenshot**:
- SSH server accessible from VM, OR
- GUI automation that can click host → wait for connection → close shell tab, OR  
- Desktop environment where Tauri app can be manually tested

**Alternative**: Manual QA testing can verify by:
1. Opening SSH connection to any host (connection=connected, open_count=1)
2. Closing the shell tab (open_count=0, connection stays connected/green)
3. Observe green dot persists without count pill
