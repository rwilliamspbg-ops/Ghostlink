import React, { useState, useEffect, useCallback, useRef } from 'react';
import Editor, { DiffEditor, type OnMount, type Monaco } from '@monaco-editor/react';
import type * as monacoEditor from 'monaco-editor';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import {
  Folder,
  FolderOpen,
  File as FileIcon,
  Save,
  Loader,
  Sparkles,
  Wrench,
  Wand2,
  Check,
  X,
  RefreshCw,
  Zap,
  SkipForward,
} from 'lucide-react';
import { useAppStore, WorkspaceEntry } from '../store';
import { GhostlinkAPI } from '../api';

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

// Pulls the first fenced code block out of a markdown reply so a "Fix" or
// "Refactor" response can be offered as a concrete diff instead of just
// prose — the model is asked (see buildScopedPrompt below) to always answer
// with a single fenced block containing the full replacement file content.
function extractFirstCodeBlock(markdown: string): string | null {
  const match = /```[a-zA-Z0-9_-]*\r?\n([\s\S]*?)```/.exec(markdown);
  return match ? match[1].replace(/\r?\n$/, '') : null;
}

// Parses the `### FILE: <path>` + fenced-block format the multi-file
// refactor prompt (below) asks the model to reply in, into path -> proposed
// content pairs. Deliberately tolerant: a file the model didn't mention
// (because it decided no change was needed) just won't appear here.
function parseMultiFileReply(content: string): Map<string, string> {
  const map = new Map<string, string>();
  const re = /###\s*FILE:\s*(.+?)\r?\n```[a-zA-Z0-9_-]*\r?\n([\s\S]*?)```/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(content))) {
    map.set(match[1].trim(), match[2].replace(/\r?\n$/, ''));
  }
  return map;
}

// DiffEditor (unlike Editor) has no `path` prop to infer language from —
// only `Editor` does that. This covers the file types this repo actually
// has; anything else just falls back to plaintext (still diffs correctly,
// just without syntax colors).
const EXT_LANGUAGE: Record<string, string> = {
  ts: 'typescript', tsx: 'typescript', js: 'javascript', jsx: 'javascript',
  rs: 'rust', go: 'go', py: 'python', json: 'json', toml: 'toml',
  yaml: 'yaml', yml: 'yaml', md: 'markdown', css: 'css', html: 'html',
  sh: 'shell', ps1: 'powershell', sql: 'sql',
};
function guessLanguage(path: string | null): string | undefined {
  if (!path) return undefined;
  const ext = path.split('.').pop()?.toLowerCase();
  return ext ? EXT_LANGUAGE[ext] : undefined;
}

type ActionKind = 'explain' | 'fix' | 'refactor';

const ACTIONS: { kind: ActionKind; label: string; icon: React.ElementType }[] = [
  { kind: 'explain', label: 'Explain', icon: Sparkles },
  { kind: 'fix', label: 'Fix', icon: Wrench },
  { kind: 'refactor', label: 'Refactor', icon: Wand2 },
];

function buildScopedPrompt(kind: ActionKind, path: string, code: string, wholeFile: boolean): string {
  const scope = wholeFile ? 'the whole file' : 'the selected excerpt below';
  const header = `File: ${path}\nYou're looking at ${scope}.`;
  if (kind === 'explain') {
    return `${header}\n\nExplain what this code does, in plain terms.\n\n\`\`\`\n${code}\n\`\`\``;
  }
  const verb = kind === 'fix' ? 'Fix any bugs in' : 'Refactor';
  return (
    `${header}\n\n${verb} this code. Reply with ONLY a single fenced code block ` +
    `containing the complete replacement for ${wholeFile ? 'the file' : 'the excerpt'} — no prose before or after.\n\n` +
    `\`\`\`\n${code}\n\`\`\``
  );
}

// A minimal per-selection assistant reply — deliberately separate from the
// main Chat tab's history (ChatTab.tsx) so asking about a file doesn't
// clutter the user's actual conversation, and so this tab keeps working
// standalone without depending on ChatTab being mounted.
interface ScratchReply {
  kind: ActionKind;
  content: string;
  loading: boolean;
}

interface MultiDiffItem {
  path: string;
  original: string;
  proposed: string;
}

// Module-level (not component state) so revisiting the Editor tab within
// the same page load doesn't re-trigger indexing every time — EditorTab
// unmounts on every tab switch, so a component-local guard would reset.
let workspaceIndexAttempted = false;

const TreeNode: React.FC<{
  entry: WorkspaceEntry;
  depth: number;
  api: GhostlinkAPI;
  openPath: string | null;
  onOpenFile: (path: string) => void;
  selectedPaths: Set<string>;
  onToggleSelect: (path: string) => void;
}> = ({ entry, depth, api, openPath, onOpenFile, selectedPaths, onToggleSelect }) => {
  const [expanded, setExpanded] = useState(false);
  const [children, setChildren] = useState<WorkspaceEntry[] | null>(null);
  const [loading, setLoading] = useState(false);

  const toggle = async () => {
    if (!entry.is_dir) {
      onOpenFile(entry.path);
      return;
    }
    if (!expanded && children === null) {
      setLoading(true);
      const result = await api.getWorkspaceTree(entry.path);
      setChildren(result.entries);
      setLoading(false);
    }
    setExpanded((e) => !e);
  };

  const isOpen = entry.path === openPath;

  return (
    <div>
      <div
        className={`flex items-center gap-1 w-full pr-2 rounded-lg transition ${
          isOpen ? 'bg-blue-600/20' : 'hover:bg-slate-800'
        }`}
        style={{ paddingLeft: `${depth * 14 + 8}px` }}
      >
        {!entry.is_dir && (
          <input
            type="checkbox"
            aria-label={`Select ${entry.name} for multi-file refactor`}
            checked={selectedPaths.has(entry.path)}
            onClick={(e) => e.stopPropagation()}
            onChange={() => onToggleSelect(entry.path)}
            className="shrink-0 accent-blue-600 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          />
        )}
        <button
          onClick={toggle}
          title={entry.path}
          className={`flex items-center gap-1.5 flex-1 min-w-0 py-1 text-left text-xs transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none rounded ${
            isOpen ? 'text-blue-400' : 'text-slate-400 hover:text-slate-200'
          }`}
        >
          {entry.is_dir ? (
            loading ? (
              <Loader size={13} className="animate-spin shrink-0" aria-hidden="true" />
            ) : expanded ? (
              <FolderOpen size={13} className="shrink-0 text-amber-400" aria-hidden="true" />
            ) : (
              <Folder size={13} className="shrink-0 text-amber-400" aria-hidden="true" />
            )
          ) : (
            <FileIcon size={13} className="shrink-0" aria-hidden="true" />
          )}
          <span className="truncate">{entry.name}</span>
        </button>
      </div>
      {entry.is_dir && expanded && children && (
        <div>
          {children.map((child) => (
            <TreeNode
              key={child.path}
              entry={child}
              depth={depth + 1}
              api={api}
              openPath={openPath}
              onOpenFile={onOpenFile}
              selectedPaths={selectedPaths}
              onToggleSelect={onToggleSelect}
            />
          ))}
        </div>
      )}
    </div>
  );
};

export const EditorTab: React.FC<{ api: GhostlinkAPI }> = ({ api }) => {
  const {
    currentModel,
    editorOpenPath, editorContent, editorOriginalContent, editorPendingDiff,
    setEditorOpenFile, setEditorContent, setEditorSaved, setEditorPendingDiff,
    addToast,
  } = useAppStore();

  const [rootEntries, setRootEntries] = useState<WorkspaceEntry[]>([]);
  const [treeError, setTreeError] = useState<string | null>(null);
  const [treeLoading, setTreeLoading] = useState(false);
  const [opening, setOpening] = useState(false);
  const [saving, setSaving] = useState(false);
  const [running, setRunning] = useState<ActionKind | null>(null);
  const [reply, setReply] = useState<ScratchReply | null>(null);
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const editorContainerRef = useRef<HTMLDivElement>(null);
  const editorResizeObserverRef = useRef<ResizeObserver | null>(null);
  const diffContainerRef = useRef<HTMLDivElement>(null);
  const multiDiffContainerRef = useRef<HTMLDivElement>(null);

  // Ghost-text inline autocomplete (opt-in — see the toggle in the tree
  // header). MVP: not true fill-in-the-middle (no suffix awareness, no
  // model-specific FIM tokens) — just "continue what's before the cursor"
  // through the same chat-completion endpoint everything else here uses, so
  // latency is a real network+inference round trip, not per-keystroke-fast.
  const [autocompleteEnabled, setAutocompleteEnabled] = useState(false);
  const autocompleteEnabledRef = useRef(false);
  const editorOpenPathRef = useRef(editorOpenPath);
  const currentModelRef = useRef(currentModel);
  const inlineProviderRef = useRef<{ dispose: () => void } | null>(null);

  // Multi-file refactor (MVP): select several files in the tree, send them
  // all in one prompt, then review each proposed change one at a time
  // (Accept/Reject/Skip) — the same diff-preview idea as the single-file
  // flow above, just queued across files instead of scoped to the open one.
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [multiRunning, setMultiRunning] = useState(false);
  const [multiQueue, setMultiQueue] = useState<MultiDiffItem[] | null>(null);
  const [multiIndex, setMultiIndex] = useState(0);

  const dirty = editorOpenPath !== null && editorContent !== editorOriginalContent;

  useEffect(() => {
    autocompleteEnabledRef.current = autocompleteEnabled;
  }, [autocompleteEnabled]);
  useEffect(() => {
    editorOpenPathRef.current = editorOpenPath;
  }, [editorOpenPath]);
  useEffect(() => {
    currentModelRef.current = currentModel;
  }, [currentModel]);

  useEffect(() => {
    return () => {
      editorResizeObserverRef.current?.disconnect();
      inlineProviderRef.current?.dispose();
    };
  }, []);

  const loadRoot = useCallback(async () => {
    setTreeLoading(true);
    const result = await api.getWorkspaceTree('');
    if (result.error) {
      setTreeError(result.error);
    } else {
      setTreeError(null);
      setRootEntries(result.entries);
    }
    setTreeLoading(false);
  }, [api]);

  useEffect(() => {
    loadRoot();
  }, [loadRoot]);

  // Repo-aware chat context: best-effort, silent unless it actually indexed
  // something — most installs won't have the `rag` MCP server configured
  // (disabled by default, needs a pulled embedding model), and that's an
  // expected "skipped" outcome, not a failure worth surfacing.
  useEffect(() => {
    if (workspaceIndexAttempted) return;
    workspaceIndexAttempted = true;
    api.indexWorkspace().then((result) => {
      if (result.status === 'ok' && (result.indexed ?? 0) > 0) {
        addToast({
          type: 'success',
          message: `Indexed ${result.indexed} file(s) for repo-aware chat context${result.capped ? ' (capped)' : ''}`,
        });
      }
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api]);

  const openFile = async (path: string) => {
    setOpening(true);
    const result = await api.getWorkspaceFile(path);
    setOpening(false);
    if (result.error) {
      addToast({ type: 'error', message: result.error });
      return;
    }
    setEditorOpenFile(path, result.content);
    setReply(null);
  };

  const toggleSelectPath = (path: string) => {
    setSelectedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const handleSave = async () => {
    if (!editorOpenPath) return;
    setSaving(true);
    const result = await api.writeWorkspaceFile(editorOpenPath, editorContent);
    setSaving(false);
    if (result.success) {
      setEditorSaved();
      addToast({ type: 'success', message: `Saved ${editorOpenPath}` });
    } else {
      addToast({ type: 'error', message: result.error || 'Failed to save file' });
    }
  };

  const runAction = async (kind: ActionKind) => {
    if (!editorOpenPath) return;
    const editor = editorRef.current;
    const selection = editor?.getSelection();
    const model = editor?.getModel();
    const hasSelection = !!selection && !!model && !selection.isEmpty();
    const scopedCode = hasSelection ? model!.getValueInRange(selection!) : editorContent;

    setRunning(kind);
    setReply({ kind, content: '', loading: true });
    setEditorPendingDiff(null);

    const result = await api.sendMessage({
      message: buildScopedPrompt(kind, editorOpenPath, scopedCode, !hasSelection),
      messages: [{ role: 'user', content: buildScopedPrompt(kind, editorOpenPath, scopedCode, !hasSelection) }],
      temperature: 0.2,
      top_p: 0.9,
      top_k: 40,
      penalty: 1.1,
      max_tokens: 2048,
      system_prompt: 'You are a precise coding assistant embedded in a code editor.',
      mcp: { tools: [] },
      stream: false,
      model: currentModel === 'none' ? undefined : currentModel,
    });

    setRunning(null);

    if (!result.success) {
      setReply({ kind, content: `Error: ${result.error || 'request failed'}`, loading: false });
      return;
    }

    const content = result.data?.response ?? '';
    setReply({ kind, content, loading: false });

    if (kind !== 'explain') {
      const codeBlock = extractFirstCodeBlock(content);
      if (codeBlock) {
        // A selection-scoped edit only replaces that range in the full
        // document — splicing by offset (not just swapping in the whole
        // reply) is what lets the diff view below show the real, complete
        // before/after of the file rather than just the changed excerpt.
        const proposedFull =
          hasSelection && selection && model
            ? editorContent.slice(0, model.getOffsetAt(selection.getStartPosition())) +
              codeBlock +
              editorContent.slice(model.getOffsetAt(selection.getEndPosition()))
            : codeBlock;
        if (proposedFull !== editorContent) {
          setEditorPendingDiff({ proposed: proposedFull });
        }
      }
    }
  };

  const acceptDiff = async () => {
    if (!editorOpenPath || !editorPendingDiff) return;
    setEditorContent(editorPendingDiff.proposed);
    const result = await api.writeWorkspaceFile(editorOpenPath, editorPendingDiff.proposed);
    if (result.success) {
      setEditorSaved();
      addToast({ type: 'success', message: `Applied edit to ${editorOpenPath}` });
    } else {
      addToast({ type: 'error', message: result.error || 'Failed to write file' });
    }
    setEditorPendingDiff(null);
  };

  const rejectDiff = () => setEditorPendingDiff(null);

  const runMultiFileRefactor = async () => {
    const paths = Array.from(selectedPaths);
    if (paths.length === 0) return;
    setMultiRunning(true);

    const files = await Promise.all(
      paths.map(async (p) => {
        const result = await api.getWorkspaceFile(p);
        return { path: p, content: result.error ? null : result.content };
      })
    );
    const usable = files.filter((f): f is { path: string; content: string } => f.content !== null);
    if (usable.length === 0) {
      setMultiRunning(false);
      addToast({ type: 'error', message: 'Could not read any of the selected files' });
      return;
    }

    const prompt =
      `Refactor the following files. For EACH file, reply with a section formatted exactly as:\n` +
      `### FILE: <path>\n\`\`\`\n<complete replacement content for that file>\n\`\`\`\n\n` +
      `Only include a section for a file if it actually needs a change. No prose outside those sections.\n\n` +
      usable.map((f) => `### FILE: ${f.path}\n\`\`\`\n${f.content}\n\`\`\``).join('\n\n');

    const result = await api.sendMessage({
      message: prompt,
      messages: [{ role: 'user', content: prompt }],
      temperature: 0.2,
      top_p: 0.9,
      top_k: 40,
      penalty: 1.1,
      max_tokens: 4096,
      system_prompt: 'You are a precise coding assistant refactoring multiple files at once.',
      mcp: { tools: [] },
      stream: false,
      model: currentModel === 'none' ? undefined : currentModel,
    });

    setMultiRunning(false);

    if (!result.success) {
      addToast({ type: 'error', message: result.error || 'Multi-file refactor request failed' });
      return;
    }

    const proposals = parseMultiFileReply(result.data?.response ?? '');
    const queue: MultiDiffItem[] = usable
      .filter((f) => proposals.has(f.path) && proposals.get(f.path) !== f.content)
      .map((f) => ({ path: f.path, original: f.content, proposed: proposals.get(f.path)! }));

    if (queue.length === 0) {
      addToast({ type: 'info', message: 'No changes proposed for the selected files' });
      return;
    }

    setMultiQueue(queue);
    setMultiIndex(0);
  };

  const advanceMultiQueue = () => {
    if (!multiQueue) return;
    if (multiIndex + 1 >= multiQueue.length) {
      setMultiQueue(null);
      setMultiIndex(0);
      setSelectedPaths(new Set());
    } else {
      setMultiIndex((i) => i + 1);
    }
  };

  const acceptMultiCurrent = async () => {
    if (!multiQueue) return;
    const item = multiQueue[multiIndex];
    const result = await api.writeWorkspaceFile(item.path, item.proposed);
    if (result.success) {
      addToast({ type: 'success', message: `Applied edit to ${item.path}` });
      // Keep the open buffer in sync if this queued file is also open.
      if (editorOpenPath === item.path) {
        setEditorContent(item.proposed);
        setEditorSaved();
      }
    } else {
      addToast({ type: 'error', message: result.error || `Failed to write ${item.path}` });
    }
    advanceMultiQueue();
  };

  const rejectMultiCurrent = () => advanceMultiQueue();

  const registerInlineCompletions = (monacoInstance: Monaco) => {
    inlineProviderRef.current?.dispose();
    inlineProviderRef.current = monacoInstance.languages.registerInlineCompletionsProvider('*', {
      async provideInlineCompletions(
        model: monacoEditor.editor.ITextModel,
        position: monacoEditor.Position,
        _context: monacoEditor.languages.InlineCompletionContext,
        token: monacoEditor.CancellationToken
      ): Promise<monacoEditor.languages.InlineCompletions> {
        if (!autocompleteEnabledRef.current) return { items: [] };

        // Monaco cancels and re-invokes this on every keystroke — sleeping
        // then checking cancellation is the standard debounce pattern for
        // inline completion providers (there's no separate "quiet period"
        // hook to wait on instead).
        await sleep(450);
        if (token.isCancellationRequested) return { items: [] };

        const path = editorOpenPathRef.current;
        if (!path) return { items: [] };

        const prefix = model.getValueInRange({
          startLineNumber: 1,
          startColumn: 1,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        });
        if (!prefix.trim()) return { items: [] };

        const prompt =
          `File: ${path}\nContinue the code exactly where it leaves off. Reply with ONLY the raw ` +
          `continuation text to insert at the cursor — no explanation, no markdown fences, don't repeat ` +
          `any of the code already shown, no leading or trailing blank lines.\n\n\`\`\`\n${prefix}\n\`\`\``;

        const modelName = currentModelRef.current;
        const result = await api.sendMessage({
          message: prompt,
          messages: [{ role: 'user', content: prompt }],
          temperature: 0.1,
          top_p: 0.9,
          top_k: 40,
          penalty: 1.1,
          max_tokens: 60,
          system_prompt: 'You are a code-completion engine. Reply with raw code only, nothing else.',
          mcp: { tools: [] },
          stream: false,
          model: modelName === 'none' ? undefined : modelName,
        });

        if (token.isCancellationRequested || !result.success) return { items: [] };

        const suggestion = (result.data?.response ?? '')
          .replace(/^```[a-zA-Z0-9_-]*\r?\n?/, '')
          .replace(/```\s*$/, '')
          .replace(/^\r?\n+/, '')
          .replace(/\r?\n+$/, '');
        if (!suggestion) return { items: [] };

        return {
          items: [
            {
              insertText: suggestion,
              range: new monacoInstance.Range(position.lineNumber, position.column, position.lineNumber, position.column),
            },
          ],
        };
      },
      freeInlineCompletions() {},
    });
  };

  const currentMultiItem = multiQueue ? multiQueue[multiIndex] : null;

  return (
    <div className="flex h-full bg-slate-950">
      {/* File tree */}
      <div className="w-64 shrink-0 border-r border-slate-900 flex flex-col">
        <div className="flex items-center justify-between px-3 py-3 border-b border-slate-900">
          <h3 className="text-xs font-bold text-slate-400 uppercase tracking-wider">Workspace</h3>
          <div className="flex items-center gap-1">
            <button
              onClick={() => setAutocompleteEnabled((v) => !v)}
              aria-pressed={autocompleteEnabled}
              aria-label="Ghost-text autocomplete"
              title="Ghost-text autocomplete (experimental — fires a real completion request as you pause typing)"
              className={`p-1 rounded-lg transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
                autocompleteEnabled ? 'bg-blue-600/20 text-blue-400' : 'text-slate-500 hover:text-white hover:bg-slate-900'
              }`}
            >
              <Zap size={13} aria-hidden="true" />
            </button>
            <button
              onClick={loadRoot}
              aria-label="Refresh file tree"
              className="p-1 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
            >
              <RefreshCw size={13} className={treeLoading ? 'animate-spin' : ''} aria-hidden="true" />
            </button>
          </div>
        </div>
        {selectedPaths.size > 0 && (
          <div className="px-3 py-2 border-b border-slate-900 flex items-center justify-between gap-2 bg-slate-900/40">
            <span className="text-[11px] text-slate-400">{selectedPaths.size} selected</span>
            <button
              onClick={runMultiFileRefactor}
              disabled={multiRunning}
              className="flex items-center gap-1 px-2 py-1 rounded-lg text-[11px] font-bold bg-purple-600 hover:bg-purple-500 text-white disabled:opacity-40 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
            >
              {multiRunning ? <Loader size={11} className="animate-spin" aria-hidden="true" /> : <Wand2 size={11} aria-hidden="true" />}
              Refactor Selected
            </button>
          </div>
        )}
        <div className="flex-1 overflow-y-auto py-2" tabIndex={0} role="region" aria-label="Workspace file tree">
          {treeError && <div className="px-3 py-2 text-xs text-red-400">{treeError}</div>}
          {!treeError && rootEntries.length === 0 && !treeLoading && (
            <div className="px-3 py-2 text-xs text-slate-600">Empty workspace</div>
          )}
          {rootEntries.map((entry) => (
            <TreeNode
              key={entry.path}
              entry={entry}
              depth={0}
              api={api}
              openPath={editorOpenPath}
              onOpenFile={openFile}
              selectedPaths={selectedPaths}
              onToggleSelect={toggleSelectPath}
            />
          ))}
        </div>
      </div>

      {/* Editor + assistant panel */}
      <div className="flex-1 flex flex-col min-w-0">
        <div className="flex items-center justify-between px-4 py-2 border-b border-slate-900">
          <div className="flex items-center gap-2 min-w-0">
            {editorOpenPath ? (
              <>
                <FileIcon size={14} className="text-slate-500 shrink-0" aria-hidden="true" />
                <span className="text-sm font-medium text-slate-200 truncate">{editorOpenPath}</span>
                {dirty && <span className="w-1.5 h-1.5 rounded-full bg-amber-400 shrink-0" title="Unsaved changes" />}
              </>
            ) : (
              <span className="text-sm text-slate-600">Select a file to open</span>
            )}
          </div>
          <div className="flex items-center gap-1 shrink-0">
            {ACTIONS.map(({ kind, label, icon: Icon }) => (
              <button
                key={kind}
                onClick={() => runAction(kind)}
                disabled={!editorOpenPath || running !== null}
                className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium text-slate-400 hover:text-white hover:bg-slate-800 disabled:opacity-40 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              >
                {running === kind ? <Loader size={13} className="animate-spin" aria-hidden="true" /> : <Icon size={13} aria-hidden="true" />}
                {label}
              </button>
            ))}
            <button
              onClick={handleSave}
              disabled={!editorOpenPath || !dirty || saving}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-bold bg-blue-600 text-white hover:bg-blue-500 disabled:opacity-40 disabled:cursor-not-allowed transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
            >
              {saving ? <Loader size={13} className="animate-spin" aria-hidden="true" /> : <Save size={13} aria-hidden="true" />}
              Save
            </button>
          </div>
        </div>

        <div className="flex-1 min-h-0 flex flex-col">
          {currentMultiItem ? (
            <div className="flex-1 min-h-0 flex flex-col">
              <div className="flex items-center justify-between px-4 py-2 bg-purple-500/10 border-b border-purple-500/30">
                <span className="text-xs font-bold text-purple-300">
                  Multi-file refactor — {multiIndex + 1} of {multiQueue!.length}: {currentMultiItem.path}
                </span>
                <div className="flex gap-2">
                  <button
                    onClick={acceptMultiCurrent}
                    className="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-bold bg-green-600 hover:bg-green-500 text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  >
                    <Check size={13} aria-hidden="true" /> Accept
                  </button>
                  <button
                    onClick={rejectMultiCurrent}
                    className="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-bold bg-slate-800 hover:bg-slate-700 text-slate-300 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  >
                    <X size={13} aria-hidden="true" /> Reject
                  </button>
                  <button
                    onClick={advanceMultiQueue}
                    title="Skip without applying or discarding — you can revisit it another time"
                    className="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-bold bg-slate-800 hover:bg-slate-700 text-slate-300 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  >
                    <SkipForward size={13} aria-hidden="true" /> Skip
                  </button>
                </div>
              </div>
              <div ref={multiDiffContainerRef} className="flex-1 min-h-0 relative">
                <DiffEditor
                  original={currentMultiItem.original}
                  modified={currentMultiItem.proposed}
                  language={guessLanguage(currentMultiItem.path)}
                  theme="vs-dark"
                  options={{ readOnly: true, renderSideBySide: true, minimap: { enabled: false }, automaticLayout: false }}
                  onMount={(diffEditor) => {
                    if (multiDiffContainerRef.current) {
                      const rect = multiDiffContainerRef.current.getBoundingClientRect();
                      diffEditor.layout({ width: rect.width, height: rect.height });
                    }
                    const ro = new ResizeObserver((entries) => {
                      const entry = entries[0];
                      if (!entry) return;
                      const { width, height } = entry.contentRect;
                      if (width > 0 && height > 0) diffEditor.layout({ width, height });
                    });
                    if (multiDiffContainerRef.current) ro.observe(multiDiffContainerRef.current);
                  }}
                />
              </div>
            </div>
          ) : editorPendingDiff ? (
            <div className="flex-1 min-h-0 flex flex-col">
              <div className="flex items-center justify-between px-4 py-2 bg-amber-500/10 border-b border-amber-500/30">
                <span className="text-xs font-bold text-amber-400">Review proposed change (original vs. suggested)</span>
                <div className="flex gap-2">
                  <button
                    onClick={acceptDiff}
                    className="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-bold bg-green-600 hover:bg-green-500 text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  >
                    <Check size={13} aria-hidden="true" /> Accept
                  </button>
                  <button
                    onClick={rejectDiff}
                    className="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-bold bg-slate-800 hover:bg-slate-700 text-slate-300 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  >
                    <X size={13} aria-hidden="true" /> Reject
                  </button>
                </div>
              </div>
              <div ref={diffContainerRef} className="flex-1 min-h-0 relative">
                <DiffEditor
                  original={editorContent}
                  modified={editorPendingDiff.proposed}
                  language={guessLanguage(editorOpenPath)}
                  theme="vs-dark"
                  options={{ readOnly: true, renderSideBySide: true, minimap: { enabled: false }, automaticLayout: false }}
                  onMount={(diffEditor) => {
                    // Same ResizeObserver-driven layout as the main Editor
                    // below — see its onMount comment for why. The immediate
                    // explicit call covers the initial paint; the observer
                    // covers the pane being resized afterward.
                    if (diffContainerRef.current) {
                      const rect = diffContainerRef.current.getBoundingClientRect();
                      diffEditor.layout({ width: rect.width, height: rect.height });
                    }
                    const ro = new ResizeObserver((entries) => {
                      const entry = entries[0];
                      if (!entry) return;
                      const { width, height } = entry.contentRect;
                      if (width > 0 && height > 0) diffEditor.layout({ width, height });
                    });
                    if (diffContainerRef.current) ro.observe(diffContainerRef.current);
                  }}
                />
              </div>
            </div>
          ) : editorOpenPath ? (
            <div ref={editorContainerRef} className="flex-1 min-h-0 relative">
              <Editor
                value={editorContent}
                path={editorOpenPath}
                theme="vs-dark"
                onChange={(value) => setEditorContent(value ?? '')}
                onMount={(editorInstance, monacoInstance) => {
                  editorRef.current = editorInstance;
                  registerInlineCompletions(monacoInstance);
                  // Monaco's own `automaticLayout` measures its container via
                  // a ResizeObserver *it* attaches — inside this nested flex
                  // layout (mounted right after an async file fetch, in a
                  // conditionally-rendered pane) that first measurement can
                  // land on a bogus near-zero size, and since the container
                  // never actually changes size afterward, Monaco's own
                  // observer never fires again to correct it. Driving
                  // `layout()` from our own ResizeObserver on the wrapper div
                  // (which we've confirmed reports the real size) sidesteps
                  // that entirely.
                  editorResizeObserverRef.current?.disconnect();
                  if (editorContainerRef.current) {
                    const rect = editorContainerRef.current.getBoundingClientRect();
                    editorInstance.layout({ width: rect.width, height: rect.height });
                  }
                  const ro = new ResizeObserver((entries) => {
                    const entry = entries[0];
                    if (!entry) return;
                    const { width, height } = entry.contentRect;
                    if (width > 0 && height > 0) editorInstance.layout({ width, height });
                  });
                  if (editorContainerRef.current) ro.observe(editorContainerRef.current);
                  editorResizeObserverRef.current = ro;
                }}
                options={{ minimap: { enabled: false }, fontSize: 13, automaticLayout: false }}
              />
            </div>
          ) : (
            <div className="flex-1 flex items-center justify-center text-slate-600 text-sm">
              {opening ? <Loader size={20} className="animate-spin" aria-hidden="true" /> : 'No file open'}
            </div>
          )}

          {reply && (
            <div className="max-h-64 overflow-y-auto border-t border-slate-900 px-4 py-3 bg-slate-900/40">
              <div className="flex items-center gap-2 mb-1.5">
                <Sparkles size={13} className="text-blue-400" aria-hidden="true" />
                <span className="text-xs font-bold text-slate-300 capitalize">{reply.kind} result</span>
                {reply.loading && <Loader size={12} className="animate-spin text-blue-500" aria-hidden="true" />}
              </div>
              <div className="prose prose-invert prose-sm max-w-none break-words text-sm">
                <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]}>
                  {reply.content || (reply.loading ? 'Thinking…' : '')}
                </ReactMarkdown>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
