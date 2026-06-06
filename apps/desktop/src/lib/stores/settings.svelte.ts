import { api } from '../api';
import type { AppConfig } from '../types';
import { DEFAULT_CONFIG } from '../types';

class SettingsStore {
  config = $state<AppConfig>({ ...DEFAULT_CONFIG });
  dirty = $state(false);
  loading = $state(false);

  private snapshot: string = '';

  async load() {
    this.loading = true;
    try {
      this.config = await api.getConfig();
      this.snapshot = JSON.stringify(this.config);
      this.dirty = false;
    } finally {
      this.loading = false;
    }
  }

  patch<K extends keyof AppConfig>(section: K, partial: Partial<AppConfig[K]>) {
    this.config = {
      ...this.config,
      [section]: { ...(this.config[section] as object), ...partial },
    };
    this.dirty = JSON.stringify(this.config) !== this.snapshot;
  }

  async save() {
    await api.saveConfig(this.config);
    this.snapshot = JSON.stringify(this.config);
    this.dirty = false;
  }

  async reset() {
    this.config = { ...DEFAULT_CONFIG };
    this.dirty = true;
  }
}

export const settingsStore = new SettingsStore();
