/**
 * FolderPicker — modal for browsing and selecting a server-side directory
 */

export class FolderPicker {
  constructor() {
    this.overlay = null;
    this.modal = null;
    this.currentPath = null;
    this.onSelect = null;
  }

  open(onSelect) {
    this.onSelect = onSelect;
    this._build();
    this._load(null);
    document.body.appendChild(this.overlay);
    document.body.appendChild(this.modal);
    this._pathInput.focus();
  }

  close() {
    this.overlay?.remove();
    this.modal?.remove();
    this.overlay = null;
    this.modal = null;
  }

  _build() {
    // Overlay
    this.overlay = document.createElement('div');
    this.overlay.className = 'folder-picker-overlay';
    this.overlay.addEventListener('click', () => this.close());

    // Modal
    this.modal = document.createElement('div');
    this.modal.className = 'folder-picker';
    this.modal.innerHTML = `
      <div class="folder-picker-header">
        <span class="folder-picker-title">Open Folder</span>
        <button class="folder-picker-close icon-btn" aria-label="Close">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
        </button>
      </div>
      <div class="folder-picker-path-row">
        <button class="folder-picker-up icon-btn" title="Parent directory" aria-label="Go up">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m5 12 7-7 7 7"/><path d="M12 19V5"/></svg>
        </button>
        <input class="folder-picker-path-input" type="text" placeholder="/path/to/folder" spellcheck="false" />
        <button class="folder-picker-go icon-btn" title="Go to path">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
        </button>
      </div>
      <div class="folder-picker-list"></div>
      <div class="folder-picker-footer">
        <span class="folder-picker-selected-label"></span>
        <div class="folder-picker-actions">
          <button class="folder-picker-cancel">Cancel</button>
          <button class="folder-picker-open" disabled>Open</button>
        </div>
      </div>
    `;

    this._pathInput = this.modal.querySelector('.folder-picker-path-input');
    this._list = this.modal.querySelector('.folder-picker-list');
    this._openBtn = this.modal.querySelector('.folder-picker-open');
    this._selectedLabel = this.modal.querySelector('.folder-picker-selected-label');

    this.modal.querySelector('.folder-picker-close').addEventListener('click', () => this.close());
    this.modal.querySelector('.folder-picker-cancel').addEventListener('click', () => this.close());

    this.modal.querySelector('.folder-picker-up').addEventListener('click', () => {
      if (this.currentPath) {
        const parts = this.currentPath.split('/').filter(Boolean);
        parts.pop();
        this._load('/' + parts.join('/') || '/');
      }
    });

    this.modal.querySelector('.folder-picker-go').addEventListener('click', () => {
      const p = this._pathInput.value.trim();
      if (p) this._load(p);
    });

    this._pathInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        const p = this._pathInput.value.trim();
        if (p) this._load(p);
      }
      if (e.key === 'Escape') this.close();
    });

    this._openBtn.addEventListener('click', () => {
      if (this.currentPath) {
        this.onSelect?.(this.currentPath);
        this.close();
      }
    });
  }

  async _load(dirPath) {
    this._list.innerHTML = '<div class="folder-picker-loading">Loading…</div>';

    try {
      const url = dirPath
        ? `/api/files?path=${encodeURIComponent(dirPath)}`
        : '/api/files';
      const res = await fetch(url);
      const data = await res.json();

      if (data.error) {
        this._list.innerHTML = `<div class="folder-picker-loading">${data.error}</div>`;
        return;
      }

      this.currentPath = data.path;
      this._pathInput.value = data.path;

      // Enable "Open" for current directory
      this._openBtn.disabled = false;
      this._selectedLabel.textContent = data.path;

      // Render only directories
      const dirs = (data.items || []).filter(i => i.isDirectory);
      this._list.innerHTML = '';

      if (dirs.length === 0) {
        this._list.innerHTML = '<div class="folder-picker-loading">No subdirectories</div>';
        return;
      }

      for (const dir of dirs) {
        const el = document.createElement('div');
        el.className = 'folder-picker-item';
        el.innerHTML = `
          <svg class="folder-picker-item-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/></svg>
          <span class="folder-picker-item-name">${this._escape(dir.name)}</span>
          <svg class="folder-picker-item-chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
        `;

        // Single click → select this dir as target
        el.addEventListener('click', () => {
          this._list.querySelectorAll('.folder-picker-item').forEach(i => i.classList.remove('selected'));
          el.classList.add('selected');
          this.currentPath = dir.path;
          this._pathInput.value = dir.path;
          this._openBtn.disabled = false;
          this._selectedLabel.textContent = dir.path;
        });

        // Double click → navigate into
        el.addEventListener('dblclick', (e) => {
          e.preventDefault();
          this._load(dir.path);
        });

        this._list.appendChild(el);
      }
    } catch {
      this._list.innerHTML = '<div class="folder-picker-loading">Failed to load directory</div>';
    }
  }

  _escape(text) {
    const d = document.createElement('div');
    d.textContent = text;
    return d.innerHTML;
  }
}
