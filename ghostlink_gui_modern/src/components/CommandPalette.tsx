import React, { useState, useEffect, useRef, useMemo } from 'react';
import { motion, AnimatePresence, useReducedMotion } from 'framer-motion';
import {
  Search,
  MessageSquare,
  Database,
  BarChart3,
  Clock,
  Network,
  Shield,
  Settings,
  Plug,
  FileCode,
  CornerDownLeft,
  Activity,
  type LucideIcon,
} from 'lucide-react';
import { useAppStore } from '../store';

export interface NavTab {
  label: string;
  icon: LucideIcon;
  id: number;
}

// Single source of truth for the sidebar nav + command palette "go to" entries
// so the two never drift out of sync.
// eslint-disable-next-line react-refresh/only-export-components
export const NAV_TABS: NavTab[] = [
  { label: 'Chat', icon: MessageSquare, id: 0 },
  { label: 'Editor', icon: FileCode, id: 8 },
  { label: 'Models', icon: Database, id: 1 },
  { label: 'Metrics', icon: BarChart3, id: 2 },
  { label: 'Sessions', icon: Clock, id: 3 },
  { label: 'Workers', icon: Network, id: 4 },
  { label: 'MCP', icon: Plug, id: 7 },
  { label: 'Security', icon: Shield, id: 5 },
  { label: 'Settings', icon: Settings, id: 6 },
];

export interface Command {
  id: string;
  label: string;
  hint?: string;
  icon: LucideIcon;
  action: () => void;
}

export const CommandPalette: React.FC = () => {
  const { setActiveTab, setChatMessages } = useAppStore();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [highlight, setHighlight] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const triggerRef = useRef<HTMLElement | null>(null);
  const shouldReduceMotion = useReducedMotion();

  const commands = useMemo<Command[]>(
    () => [
      // Phase 1: Health & Security
      {
        id: 'retry-health',
        label: 'Retry health check',
        hint: 'Probe backend API health endpoint',
        icon: Activity,
        action: () => {
          window.dispatchEvent(new CustomEvent('retry-health-check'));
          setActiveTab(5);
        },
      },
      {
        id: 'set-api-key',
        label: 'Set API Key',
        hint: 'Configure Bearer token in Security tab',
        icon: Shield,
        action: () => setActiveTab(5),
      },
      {
        id: 'open-health',
        label: 'Open health status',
        hint: 'View system health & security details',
        icon: Shield,
        action: () => setActiveTab(5),
      },

      // Phase 2: Models
      {
        id: 'download-models',
        label: 'Download models',
        hint: 'Search and download models from Hugging Face Hub',
        icon: Database,
        action: () => setActiveTab(1),
      },
      {
        id: 'load-model',
        label: 'Load model',
        hint: 'View local models and load into memory',
        icon: Database,
        action: () => setActiveTab(1),
      },
      {
        id: 'unload-model',
        label: 'Unload model',
        hint: 'Unload active model from memory',
        icon: Database,
        action: () => setActiveTab(1),
      },

      // Phase 3: Chat & Presets
      {
        id: 'new-chat',
        label: 'New Chat',
        hint: 'Clear the conversation and start fresh · Ctrl+Shift+O',
        icon: MessageSquare,
        action: () => {
          const messages = useAppStore.getState().chatMessages;
          if (
            messages.length === 0 ||
            window.confirm(
              'Are you sure you want to clear the current chat? This will permanently delete your active conversation.'
            )
          ) {
            setChatMessages([]);
            setActiveTab(0);
          }
        },
      },
      {
        id: 'search-threads',
        label: 'Search threads',
        hint: 'View saved chat threads in Chat tab',
        icon: MessageSquare,
        action: () => setActiveTab(0),
      },
      {
        id: 'prompt-presets',
        label: 'System prompt presets',
        hint: 'Manage system prompt presets in Chat tab',
        icon: MessageSquare,
        action: () => setActiveTab(0),
      },

      // Phase 4: Cluster & Workers
      {
        id: 'discover-peers',
        label: 'Discover LAN peers',
        hint: 'Scan local network for available cluster workers',
        icon: Network,
        action: () => {
          setActiveTab(4);
          window.dispatchEvent(new CustomEvent('discover-lan-peers'));
        },
      },
      {
        id: 'use-other-machines',
        label: 'Use other machines',
        hint: 'Toggle offloading models to cluster workers',
        icon: Network,
        action: () => {
          setActiveTab(4);
          window.dispatchEvent(new CustomEvent('toggle-cluster-offload'));
        },
      },

      // Phase 5: MCP & Workspace
      {
        id: 'enable-calculator',
        label: 'Enable Calculator / MCP',
        hint: 'Enable in-tree calculator MCP server',
        icon: Plug,
        action: () => {
          setActiveTab(7);
          window.dispatchEvent(new CustomEvent('enable-mcp-calculator'));
        },
      },
      {
        id: 'index-workspace',
        label: 'Index workspace',
        hint: 'Scan and index workspace files in Editor tab',
        icon: FileCode,
        action: () => {
          setActiveTab(8);
          window.dispatchEvent(new CustomEvent('index-workspace-files'));
        },
      },
      {
        id: 'toggle-workspace-context',
        label: 'Toggle workspace context',
        hint: 'Toggle RAG workspace context in Editor tab',
        icon: FileCode,
        action: () => {
          setActiveTab(8);
          window.dispatchEvent(new CustomEvent('toggle-workspace-context'));
        },
      },

      ...NAV_TABS.map((tab) => ({
        id: `goto-${tab.id}`,
        label: `Go to ${tab.label}`,
        icon: tab.icon,
        action: () => setActiveTab(tab.id),
      })),
    ],
    [setActiveTab, setChatMessages]
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter((c) => c.label.toLowerCase().includes(q));
  }, [commands, query]);

  useEffect(() => {
    setHighlight(0);
  }, [query, open]);

  useEffect(() => {
    if (!open || !filtered[highlight]) return;
    const activeEl = listRef.current?.children[highlight] as HTMLElement | undefined;
    activeEl?.scrollIntoView?.({ block: 'nearest' });
  }, [highlight, open, filtered]);

  // Global keyboard shortcuts & custom trigger events — the only place any of these are wired up.
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setOpen((o) => {
          if (!o) triggerRef.current = document.activeElement as HTMLElement;
          return !o;
        });
        return;
      }
      if (e.key === 'Escape') {
        setOpen((o) => (o ? false : o));
        return;
      }
      // Ctrl/Cmd+Shift+O for "new chat"
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'o') {
        e.preventDefault();
        const messages = useAppStore.getState().chatMessages;
        if (
          messages.length === 0 ||
          window.confirm(
            'Are you sure you want to clear the current chat? This will permanently delete your active conversation.'
          )
        ) {
          setChatMessages([]);
          setActiveTab(0);
        }
      }
    };
    const handleOpenPalette = () => {
      setOpen((o) => {
        if (!o) triggerRef.current = document.activeElement as HTMLElement;
        return true;
      });
    };
    document.addEventListener('keydown', handleKeyDown);
    window.addEventListener('open-command-palette', handleOpenPalette);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('open-command-palette', handleOpenPalette);
    };
  }, [setActiveTab, setChatMessages]);

  useEffect(() => {
    if (open) {
      setQuery('');
      requestAnimationFrame(() => inputRef.current?.focus());
    } else {
      triggerRef.current?.focus();
      triggerRef.current = null;
    }
  }, [open]);

  const runCommand = (cmd: Command) => {
    cmd.action();
    setOpen(false);
  };

  const handleInputKeyDown = (e: React.KeyboardEvent) => {
    if (filtered.length === 0) return;
    if (e.key === 'ArrowDown' || (e.key === 'Tab' && !e.shiftKey)) {
      e.preventDefault();
      setHighlight((h) => (h + 1) % filtered.length);
    } else if (e.key === 'ArrowUp' || (e.key === 'Tab' && e.shiftKey)) {
      e.preventDefault();
      setHighlight((h) => (h - 1 + filtered.length) % filtered.length);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const cmd = filtered[highlight];
      if (cmd) runCommand(cmd);
    }
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.15 }}
          className="fixed inset-0 z-[100] flex items-start justify-center pt-[15vh] bg-slate-950/70 backdrop-blur-sm"
          onClick={() => setOpen(false)}
        >
          <motion.div
            initial={shouldReduceMotion ? { opacity: 0 } : { opacity: 0, scale: 0.96, y: -8 }}
            animate={shouldReduceMotion ? { opacity: 1 } : { opacity: 1, scale: 1, y: 0 }}
            exit={shouldReduceMotion ? { opacity: 0 } : { opacity: 0, scale: 0.96, y: -8 }}
            transition={{ duration: shouldReduceMotion ? 0.01 : 0.15, ease: [0.16, 1, 0.3, 1] }}
            role="dialog"
            aria-modal="true"
            aria-label="Command palette"
            onClick={(e) => e.stopPropagation()}
            className="w-full max-w-lg mx-4 bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl overflow-hidden"
          >
            <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-800 focus-within:ring-2 focus-within:ring-inset focus-within:ring-blue-500">
              <Search size={16} className="text-slate-500 shrink-0" aria-hidden="true" />
              <input
                ref={inputRef}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={handleInputKeyDown}
                placeholder="Type a command or search..."
                aria-label="Search commands"
                role="combobox"
                aria-expanded="true"
                aria-controls="command-palette-list"
                aria-activedescendant={filtered[highlight] ? `command-${filtered[highlight].id}` : undefined}
                className="w-full bg-transparent border-none text-sm text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-0"
              />
              <kbd className="text-[10px] text-slate-500 bg-slate-950 px-1.5 py-0.5 rounded border border-slate-700">Esc</kbd>
            </div>
            <div className="sr-only" role="status" aria-live="polite">
              {filtered.length === 0
                ? 'No matching commands'
                : `${filtered.length} command${filtered.length === 1 ? '' : 's'} found`}
            </div>
            <ul
              ref={listRef}
              id="command-palette-list"
              role="listbox"
              aria-label="Commands"
              className="max-h-80 overflow-y-auto p-2"
            >
              {filtered.length === 0 ? (
                <li className="px-3 py-6 text-center text-sm text-slate-500">No matching commands</li>
              ) : (
                filtered.map((cmd, i) => {
                  const Icon = cmd.icon;
                  return (
                    // eslint-disable-next-line jsx-a11y/click-events-have-key-events -- combobox/listbox pattern
                    <li
                      key={cmd.id}
                      id={`command-${cmd.id}`}
                      role="option"
                      aria-selected={i === highlight}
                      onMouseEnter={() => setHighlight(i)}
                      onClick={() => runCommand(cmd)}
                      className={`flex items-center gap-3 px-3 py-2.5 rounded-xl text-sm cursor-pointer transition ${
                        i === highlight ? 'bg-blue-600/10 text-blue-400' : 'text-slate-300'
                      }`}
                    >
                      <Icon size={16} className={i === highlight ? 'text-blue-400' : 'text-slate-500'} />
                      <div className="flex flex-col min-w-0">
                        <span className="truncate">{cmd.label}</span>
                        {cmd.hint && <span className="text-[10px] text-slate-500 truncate">{cmd.hint}</span>}
                      </div>
                      {i === highlight && (
                        <CornerDownLeft size={12} className="ml-auto text-slate-600 shrink-0" aria-hidden="true" />
                      )}
                    </li>
                  );
                })
              )}
            </ul>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
};
