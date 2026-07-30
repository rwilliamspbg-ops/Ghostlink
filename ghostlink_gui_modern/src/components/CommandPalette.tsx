import React, { useEffect, useMemo, useRef, useState } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
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

interface Command {
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
  const triggerRef = useRef<HTMLElement | null>(null);

  const commands = useMemo<Command[]>(
    () => [
      {
        id: 'new-chat',
        label: 'New Chat',
        hint: 'Clear the conversation and start fresh · Ctrl+Shift+O',
        icon: MessageSquare,
        action: () => {
          const messages = useAppStore.getState().chatMessages;
          if (messages.length === 0 || window.confirm('Are you sure you want to clear the current chat? This will permanently delete your active conversation.')) {
            setChatMessages([]);
            setActiveTab(0);
          }
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

  // Global keyboard shortcuts — the only place any of these are wired up.
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
      // Ctrl/Cmd+N is reserved by most browsers for "new window" and can't
      // reliably be intercepted — Ctrl/Cmd+Shift+O is the same convention
      // ChatGPT uses for "new chat" and is actually interceptable.
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'o') {
        e.preventDefault();
        const messages = useAppStore.getState().chatMessages;
        if (messages.length === 0 || window.confirm('Are you sure you want to clear the current chat? This will permanently delete your active conversation.')) {
          setChatMessages([]);
          setActiveTab(0);
        }
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
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
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setHighlight((h) => Math.min(h + 1, filtered.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setHighlight((h) => Math.max(h - 1, 0));
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
            initial={{ opacity: 0, scale: 0.96, y: -8 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: -8 }}
            transition={{ duration: 0.15, ease: [0.16, 1, 0.3, 1] }}
            role="dialog"
            aria-modal="true"
            aria-label="Command palette"
            onClick={(e) => e.stopPropagation()}
            className="w-full max-w-lg mx-4 bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl overflow-hidden"
          >
        <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-800">
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
        <ul id="command-palette-list" role="listbox" aria-label="Commands" className="max-h-80 overflow-y-auto p-2">
          {filtered.length === 0 ? (
            <li className="px-3 py-6 text-center text-sm text-slate-500">No matching commands</li>
          ) : (
            filtered.map((cmd, i) => {
              const Icon = cmd.icon;
              return (
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
                  {i === highlight && <CornerDownLeft size={12} className="ml-auto text-slate-600 shrink-0" aria-hidden="true" />}
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
