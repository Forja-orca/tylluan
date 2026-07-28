import { create } from 'zustand';

interface Toast {
  id: number;
  msg: string;
  guild?: string;
  type: 'info' | 'error';
}

interface Mention {
  id: number;
  sender: string;
  channel: string;
  message: string;
  ts: Date;
}

interface PendingGrant {
  requestId: string;
  guild: string;
  agentId: string;
  tool: string;
  blockedReason: string;
  options: string[];
}

interface AppState {
  theme: 'dark' | 'light' | 'system';
  activeTab: string;
  mountedTabs: Set<string>;
  sidebarCollapsed: boolean;
  toasts: Toast[];
  kernelUptime: number;
  coloquioUnread: number;
  activeMentions: Mention[];
  showMentionsDropdown: boolean;
  pendingGrant: PendingGrant | null;

  setTheme: (theme: 'dark' | 'light' | 'system') => void;
  setActiveTab: (tab: string) => void;
  toggleSidebar: () => void;
  addToast: (msg: string, type?: 'info' | 'error', guild?: string) => void;
  removeToast: (id: number) => void;
  incrementUptime: () => void;
  setColoquioUnread: (n: number | ((prev: number) => number)) => void;
  addMention: (mention: Omit<Mention, 'id' | 'ts'>) => void;
  clearMentions: () => void;
  setShowMentionsDropdown: (show: boolean) => void;
  setPendingGrant: (grant: PendingGrant | null) => void;
}

const getInitialTheme = (): 'dark' | 'light' | 'system' => {
  try {
    return (localStorage.getItem('tylluan_theme') as 'dark' | 'light' | 'system') || 'system';
  } catch {
    return 'system';
  }
};

const getInitialTab = (): string => {
  try {
    return localStorage.getItem('tylluan_active_tab') || 'overview';
  } catch {
    return 'overview';
  }
};

export const useAppStore = create<AppState>((set) => ({
  theme: getInitialTheme(),
  activeTab: getInitialTab(),
  mountedTabs: new Set(['overview', getInitialTab()]),
  sidebarCollapsed: false,
  toasts: [],
  kernelUptime: 0,
  coloquioUnread: 0,
  activeMentions: [],
  showMentionsDropdown: false,
  pendingGrant: null,

  setTheme: (theme) => {
    localStorage.setItem('tylluan_theme', theme);
    set({ theme });
  },

  setActiveTab: (tab) => {
    localStorage.setItem('tylluan_active_tab', tab);
    set((state) => ({
      activeTab: tab,
      mountedTabs: new Set(state.mountedTabs).add(tab),
    }));
  },

  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),

  addToast: (msg, type = 'info', guild) => {
    const id = Date.now();
    set((state) => ({
      toasts: [{ id, msg, type, guild }, ...state.toasts].slice(0, 5),
    }));
    setTimeout(() => {
      set((state) => ({
        toasts: state.toasts.filter((t) => t.id !== id),
      }));
    }, 5000);
  },

  removeToast: (id) =>
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    })),

  incrementUptime: () => set((state) => ({ kernelUptime: state.kernelUptime + 1 })),

  setColoquioUnread: (n) =>
    set((state) => ({
      coloquioUnread: typeof n === 'function' ? n(state.coloquioUnread) : n,
    })),

  addMention: (mention) =>
    set((state) => ({
      activeMentions: [
        { ...mention, id: Date.now(), ts: new Date() },
        ...state.activeMentions,
      ].slice(0, 10),
    })),

  clearMentions: () => set({ activeMentions: [] }),

  setShowMentionsDropdown: (show) => set({ showMentionsDropdown: show }),

  setPendingGrant: (grant) => set({ pendingGrant: grant }),
}));
