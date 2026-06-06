type ToastKind = 'info' | 'success' | 'error';

interface ToastItem {
  id: number;
  kind: ToastKind;
  text: string;
}

let nextId = 0;

class ToastStore {
  items = $state<ToastItem[]>([]);

  show(kind: ToastKind, text: string, ms = 3000) {
    const id = nextId++;
    this.items = [...this.items, { id, kind, text }];
    setTimeout(() => {
      this.items = this.items.filter((t) => t.id !== id);
    }, ms);
  }

  info(text: string) { this.show('info', text); }
  success(text: string) { this.show('success', text); }
  error(text: string) { this.show('error', text, 5000); }
}

export const toast = new ToastStore();
