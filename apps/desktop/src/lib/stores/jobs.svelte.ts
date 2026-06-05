// Reactive store для задач. Svelte 5 runes-based.
import { api } from '../api';
import type { BackendEvent, Job, JobId } from '../types';

class JobsStore {
  jobs = $state<Job[]>([]);
  activeId = $state<JobId | null>(null);
  loading = $state(false);

  byId(id: JobId): Job | undefined {
    return this.jobs.find((j) => j.id === id);
  }

  active(): Job | undefined {
    return this.activeId ? this.byId(this.activeId) : undefined;
  }

  async init() {
    this.loading = true;
    try {
      this.jobs = await api.listJobs();
    } finally {
      this.loading = false;
    }
    api.onJobEvent((ev) => this.apply(ev));
  }

  apply(ev: BackendEvent) {
    switch (ev.kind) {
      case 'job:created':
        this.jobs = [ev.job, ...this.jobs.filter((j) => j.id !== ev.job.id)];
        if (!this.activeId) this.activeId = ev.job.id;
        break;
      case 'job:updated': {
        this.jobs = this.jobs.map((j) => (j.id === ev.id ? { ...j, ...ev.patch } : j));
        break;
      }
      case 'job:log': {
        // В нашей модели лог идёт встроенно, но можно накапливать здесь
        break;
      }
      case 'download:progress': {
        this.jobs = this.jobs.map((j) =>
          j.id === ev.id
            ? { ...j, stage: 'downloading', progress: { pct: ev.pct, label: ev.label, speed: ev.speed, eta: ev.eta } }
            : j,
        );
        break;
      }
      case 'job:done':
        this.jobs = this.jobs.map((j) =>
          j.id === ev.id
            ? { ...j, stage: 'done', progress: { pct: 1, label: 'Готово' }, transcriptPath: ev.transcriptPath, transcriptPreview: ev.preview, finishedAt: new Date().toISOString() }
            : j,
        );
        break;
      case 'job:failed':
        this.jobs = this.jobs.map((j) =>
          j.id === ev.id ? { ...j, stage: 'failed', error: ev.error, finishedAt: new Date().toISOString() } : j,
        );
        break;
    }
  }

  async add(url: string) {
    const id = await api.enqueueUrl(url);
    if (!this.activeId) this.activeId = id;
  }

  async cancel(id: JobId) {
    await api.cancelJob(id);
  }

  select(id: JobId | null) {
    this.activeId = id;
  }
}

export const jobsStore = new JobsStore();
