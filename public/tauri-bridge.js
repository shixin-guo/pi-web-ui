/**
 * Tauri bridge — exposes native IPC to the existing pi-web-ui frontend.
 * Only active when running inside the Tauri desktop app (window.__TAURI__ is set).
 *
 * Instead of hack approaches (osascript, no-op session switches), we use:
 *   - newSession(port)          → RPC cmd to pi: create new session
 *   - switchSession(port, path) → RPC cmd to pi: switch to session file
 *   - openWorkspace(cwd)        → spawn new pi process + new OS window
 *   - pickFolder()              → native macOS folder picker
 *   - stopInstance(port)        → kill a pi instance
 */

(function () {
  const tauriCore = window.__TAURI__?.core;
  if (!tauriCore || typeof tauriCore.invoke !== 'function') {
    console.error('[tauri-bridge] Tauri core API unavailable');
    return;
  }
  const invoke = (cmd, args) => tauriCore.invoke(cmd, args);

  // Detect current window's port from the URL
  function currentPort() {
    return parseInt(location.port) || 3001;
  }

  window.tauriNative = {
    isTauri: true,

    pickFolder: () =>
      invoke('cmd_pick_folder'),

    openWorkspace: (cwd, options = {}) =>
      invoke('cmd_open_workspace', {
        cwd,
        sessionPath: options.sessionPath ?? null,
        forceNewSession: options.forceNewSession ?? true,
        openWindow: options.openWindow ?? true,
        waitForSessions: options.waitForSessions ?? false,
      }),

    newSession: (port) =>
      invoke('cmd_new_session', { port: port ?? currentPort() }),

    switchSession: (sessionPath, port) =>
      invoke('cmd_switch_session', { port: port ?? currentPort(), sessionPath }),

    stopInstance: (port) =>
      invoke('cmd_stop_instance', { port: port ?? currentPort() }),

    getPiVersion: () =>
      invoke('cmd_get_pi_version'),

    currentPort,
  };

  console.log('[tauri-bridge] Native APIs ready on port', currentPort());
})();
