import React, { useState, useEffect, useMemo, useRef, useCallback } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import {
  Send,
  Loader,
  Plus,
  ChevronDown,
  Wand2,
  X,
  User,
  Bot,
  Copy,
  ThumbsUp,
  ThumbsDown,
  RotateCcw,
  Volume2,
  Check,
  Save,
  FolderOpen,
  Trash2,
  Database,
  Wrench,
  ShieldAlert,
  GitCompare,
  Pencil,
  Mic,
  Paperclip,
  FileText,
} from 'lucide-react';
import { useAppStore, ChatMessage } from '../store';
import { GhostlinkAPI } from '../api';
import { useInferenceEngines } from '../hooks/useInferenceEngines';

type Message = ChatMessage;

// Browser-native speech-to-text (Web Speech API) — not in TS's DOM lib under
// the vendor-prefixed name Chrome/Edge actually ship it under, and Firefox/
// Safari don't implement it at all, hence the `any`-typed feature-detect
// rather than a proper interface. Cloud-backed (needs internet), unlike the
// rest of this app's local-first inference — a real tradeoff, not hidden:
// the mic button below simply doesn't render where this is undefined.
// Resolved lazily (not a module-level constant) so it reflects `window` at
// call time rather than whatever it was when this module first loaded —
// also what makes it mockable in tests via `vi.stubGlobal`.
function getSpeechRecognitionCtor(): any {
  if (typeof window === 'undefined') return undefined;
  return (window as any).SpeechRecognition ?? (window as any).webkitSpeechRecognition;
}

interface Session {
  id: string;
  name?: string;
  model: string;
  status: string;
  throughput: number;
  latency: number;
  tokens: number;
}

interface Tool {
  name: string;
  description: string;
  enabled: boolean;
}

// Tool slot names (e.g. "git_tools", "docker_gateway") straight from
// mcp_servers.toml are snake_case and meant for config, not display. This
// used to be `tool.name.replace('_', ' ')` — lowercase, and only fixing the
// *first* underscore (harmless today since every current slot has at most
// one, but a latent bug for the next multi-word one) — while every other
// label in the app is Title Case.
const TOOL_LABEL_WORD_OVERRIDES: Record<string, string> = { api: 'API', rag: 'RAG', mcp: 'MCP' };
function toolDisplayName(name: string): string {
  return name
    .split('_')
    .map((word) => TOOL_LABEL_WORD_OVERRIDES[word.toLowerCase()] ?? word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
}

// Mirrors the backend's chars/4 heuristic (main.rs `estimate_tokens`) so the
// chip's number and the server's actual truncation point roughly agree —
// exact agreement isn't the point, just not being wildly off from each other.
function estimateTokens(text: string): number {
  const chars = text.trim().length;
  return chars === 0 ? 0 : Math.max(1, Math.floor(text.length / 4));
}

function getNodeText(node: React.ReactNode): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(getNodeText).join('');
  if (React.isValidElement(node)) return getNodeText((node.props as { children?: React.ReactNode }).children);
  return '';
}

// react-markdown maps a fenced code block to <pre><code class="language-x hljs">...
// rehype-highlight already syntax-colors the children by the time this renders,
// so this only adds the language label + copy button chrome around it.
const CodeBlock: React.FC<{ children?: React.ReactNode }> = ({ children, ...rest }) => {
  const [copied, setCopied] = useState(false);
  const codeEl = Array.isArray(children) ? children[0] : children;
  const lang = React.isValidElement(codeEl)
    ? /language-(\w+)/.exec((codeEl.props as { className?: string }).className || '')?.[1]
    : undefined;

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(getNodeText(children));
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch { /* noop */ }
  };

  return (
    <div className="relative group/code my-2">
      {lang && (
        <div className="absolute top-0 left-3 -translate-y-1/2 px-2 py-0.5 bg-slate-800 border border-slate-700 rounded text-[10px] font-mono text-slate-400 uppercase tracking-wider">
          {lang}
        </div>
      )}
      <button
        onClick={handleCopy}
        aria-label="Copy code"
        title="Copy code"
        className="absolute top-2 right-2 p-1.5 rounded-lg bg-slate-800/80 border border-slate-700 text-slate-400 hover:text-white hover:bg-slate-700 opacity-0 group-hover/code:opacity-100 focus-visible:opacity-100 focus:opacity-100 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none transition"
      >
        {copied ? <Check size={13} className="text-green-400" aria-hidden="true" /> : <Copy size={13} aria-hidden="true" />}
      </button>
      <pre {...rest} className="!bg-slate-950 !border !border-slate-800 !rounded-xl !p-4 overflow-x-auto text-[13px] leading-relaxed">
        {children}
      </pre>
    </div>
  );
};

// GFM tables (remark-gfm) render at their natural width, which routinely
// blows past a chat bubble's — wrap in a scroll container instead of letting
// them force the whole bubble (and page) wider or mangle-wrap every cell.
const MarkdownTable: React.FC<React.TableHTMLAttributes<HTMLTableElement>> = (props) => (
  <div className="overflow-x-auto">
    <table {...props} />
  </div>
);

// Shared knobs for both markdown renders below: tames default heading sizes
// (typography's h1/h2/h3 scale reads fine on a full-width article, but is
// oversized inside a narrow chat bubble — a one-line reply with a "# Summary"
// heading shouldn't render that heading larger than the rest of the message
// combined), tightens list/paragraph rhythm, and swaps the plugin's
// backtick-quoted inline `code` styling for a subtler background pill.
const MARKDOWN_PROSE_CLASSES =
  'max-w-none break-words ' +
  'prose-headings:font-semibold ' +
  'prose-h1:text-lg prose-h1:mt-4 prose-h1:mb-2 first:prose-h1:mt-0 ' +
  'prose-h2:text-base prose-h2:mt-4 prose-h2:mb-2 first:prose-h2:mt-0 ' +
  'prose-h3:text-sm prose-h3:mt-3 prose-h3:mb-1 first:prose-h3:mt-0 ' +
  'prose-p:leading-relaxed prose-li:my-1 prose-ul:my-2 prose-ol:my-2 ' +
  // `prose-code:` targets every <code>, inside <pre> or not — scoping to
  // `:not(pre)>code` keeps this to inline code so it can't also shrink/pill
  // the syntax-highlighted text inside fenced code blocks (CodeBlock already
  // styles those directly).
  '[&_:not(pre)>code]:before:content-none [&_:not(pre)>code]:after:content-none ' +
  '[&_:not(pre)>code]:bg-slate-800 [&_:not(pre)>code]:text-slate-200 [&_:not(pre)>code]:font-normal ' +
  '[&_:not(pre)>code]:rounded [&_:not(pre)>code]:px-1.5 [&_:not(pre)>code]:py-0.5 ' +
  '[&_:not(pre)>code]:text-[0.85em] [&_:not(pre)>code]:break-words';

// One column of a Compare Mode turn — a deliberately trimmed-down sibling of
// the full message bubble below (no tool-call cards, no pending-confirmation
// UI, no regenerate/thumbs actions) since compare turns don't use tools and
// keeping the column compact matters more than parity with the single-model
// bubble's full action set.
const CompareColumn: React.FC<{
  msg: Message;
  isLoading: boolean;
  copiedId: string | null;
  onCopy: (content: string, id: string) => void;
}> = ({ msg, isLoading, copiedId, onCopy }) => (
  <div className="flex flex-col gap-2 bg-slate-900/40 border border-slate-800 rounded-2xl p-4 min-w-0">
    <div className="flex items-center gap-2 px-1">
      <div className="w-5 h-5 bg-blue-600 rounded-sm flex items-center justify-center text-[10px] font-bold text-white uppercase shrink-0">
        {msg.model ? msg.model.substring(0, 1) : 'G'}
      </div>
      <span className="text-xs font-bold text-slate-200 truncate">
        {msg.model ? msg.model.split('/').pop() : 'Ghostlink'}
      </span>
      {isLoading && <Loader size={12} className="text-blue-500 animate-spin ml-auto shrink-0" aria-hidden="true" />}
    </div>
    <div className="text-sm leading-relaxed text-slate-200 min-h-[1.5em]">
      {msg.content ? (
        <div className={`prose prose-invert prose-sm ${MARKDOWN_PROSE_CLASSES}`}>
          <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]} components={{ pre: CodeBlock, table: MarkdownTable }}>
            {msg.content}
          </ReactMarkdown>
          {isLoading && <span className="inline-block w-[7px] h-[1em] bg-blue-400 align-text-bottom ml-0.5 animate-pulse" />}
        </div>
      ) : (
        <span className="text-slate-600 text-xs">{isLoading ? 'Thinking…' : ''}</span>
      )}
    </div>
    {msg.content && !isLoading && (
      <button
        onClick={() => onCopy(msg.content, msg.id)}
        className="self-start p-1.5 rounded-lg hover:bg-slate-800 text-slate-500 hover:text-slate-300 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none transition"
        aria-label="Copy response"
        title="Copy response"
      >
        {copiedId === msg.id ? <Check size={12} className="text-green-400" aria-hidden="true" /> : <Copy size={12} aria-hidden="true" />}
      </button>
    )}
  </div>
);

const CompareRow: React.FC<{
  a: Message;
  b?: Message;
  compareLoadingIds: Set<string>;
  copiedId: string | null;
  onCopy: (content: string, id: string) => void;
}> = ({ a, b, compareLoadingIds, copiedId, onCopy }) => (
  <div className="flex gap-4 justify-start animate-in fade-in slide-in-from-bottom-2 duration-300 w-full">
    <div className="flex-shrink-0 w-8 h-8 rounded-lg bg-purple-500/10 border border-purple-500/20 flex items-center justify-center mt-1">
      <GitCompare size={14} className="text-purple-400" aria-hidden="true" />
    </div>
    <div className="flex-1 grid grid-cols-1 md:grid-cols-2 gap-3 min-w-0">
      <CompareColumn msg={a} isLoading={compareLoadingIds.has(a.id)} copiedId={copiedId} onCopy={onCopy} />
      {b && <CompareColumn msg={b} isLoading={compareLoadingIds.has(b.id)} copiedId={copiedId} onCopy={onCopy} />}
    </div>
  </div>
);

// Attachments are read entirely client-side and inlined as fenced code
// blocks — there's no upload/multimodal endpoint, so only text-ish files
// make sense here; a 256KB cap keeps one large paste from blowing the
// conversation token budget.
const MAX_ATTACHMENT_BYTES = 256 * 1024;
const TEXT_FILE_EXTENSIONS = new Set([
  'txt', 'md', 'markdown', 'json', 'csv', 'tsv', 'log', 'yaml', 'yml', 'toml', 'ini', 'cfg', 'conf', 'env',
  'py', 'js', 'jsx', 'ts', 'tsx', 'rs', 'go', 'java', 'kt', 'c', 'h', 'cpp', 'hpp', 'cs', 'rb', 'php',
  'sh', 'bash', 'ps1', 'sql', 'css', 'scss', 'html', 'htm', 'xml', 'svg',
]);

// Pre-configured start prompts to improve immediate UX and help-seeking onboarding.
const SUGGESTIONS = [
  { text: 'Check active cluster node health', label: 'Ask about active cluster node health' },
  { text: 'Detail GGUF local model execution', label: 'Ask about GGUF local model execution' },
  { text: 'List all available capabilities', label: 'Ask about available capabilities' },
  { text: 'Show performance metrics overview', label: 'Ask about performance metrics overview' },
];

export const ChatTab: React.FC<{ api: GhostlinkAPI }> = ({ api }) => {
  const {
    currentModel, models, setCurrentModel, mcpServers, setMcpServers,
    chatMessages: messages, setChatMessages: setMessages,
    chatLoading: loading, setChatLoading: setLoading,
    chatStreamingId: streamingId, setChatStreamingId: setStreamingId,
    chatError: error, setChatError: setError,
    addToast,
    setActiveTab,
  } = useAppStore();
  const [input, setInput] = useState('');
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [isRecording, setIsRecording] = useState(false);
  const recognitionRef = useRef<any>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // What was already in the input box when recording started, and the
  // finalized (non-interim) transcript so far — combined with the current
  // interim chunk on every onresult event to rebuild the full input value
  // without duplicating text across repeated interim updates.
  const recordingBaseRef = useRef('');
  const finalTranscriptRef = useRef('');

  // Controls
  const [temperature] = useState(0.7);
  const [maxTokens] = useState(2048);
  const [systemPrompt] = useState('You are a production-grade, kernel-bypass-aware system orchestration co-pilot.');

  // Mirrors the Settings tab's Conversation Token Limit so the header chip
  // below can warn before the server has to start dropping history, not
  // just after. Loaded once — if it's changed in Settings mid-session the
  // chip catches up next time this tab mounts, which is fine for a
  // best-effort heads-up rather than an exact accounting.
  const [conversationTokenLimit, setConversationTokenLimit] = useState(3072);

  // UI State
  const [showTools, setShowTools] = useState(false);
  const [showModelSelector, setShowModelSelector] = useState(false);
  const [showSessions, setShowSessions] = useState(false);
  const [sessionName, setSessionName] = useState(`Session ${new Date().toLocaleDateString()}`);
  const [tools, setTools] = useState<Tool[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [sessionsLoading, setSessionsLoading] = useState(false);
  const { selectedEngine } = useInferenceEngines(api);
  const [confirmingId, setConfirmingId] = useState<string | null>(null);

  // File attachments: read entirely client-side and inlined into the
  // outgoing message as fenced code blocks. There's no multimodal/upload
  // path on the backend (the chat request is plain text), so this is
  // deliberately scoped to text-ish files rather than pretending to support
  // images the model will never actually see.
  const [attachments, setAttachments] = useState<{ id: string; name: string; content: string; size: number }[]>([]);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [isDraggingFile, setIsDraggingFile] = useState(false);

  // Compare Mode: send one turn to two models at once and render the
  // replies side-by-side. Deliberately scoped down from full conversation
  // branching (a tree data model + branch-navigation UI) to something
  // tractable — same idea the original spec floated ("comparison or
  // branching, if feasible"), picking the lower-cost half.
  const [compareModel, setCompareModel] = useState<string | null>(null);
  const [showCompareSelector, setShowCompareSelector] = useState(false);
  const [compareLoadingIds, setCompareLoadingIds] = useState<Set<string>>(new Set());
  const compareSelectorRef = useRef<HTMLDivElement>(null);

  // Live generation feedback: how long we've been waiting / how fast tokens
  // are arriving, so a slow prompt-eval phase doesn't look like a hang.
  const genStartRef = useRef<number | null>(null);
  const genTokenCountRef = useRef(0);
  const [genTick, setGenTick] = useState(0);

  // Screen readers can't sensibly narrate a response as it streams token by
  // token, so instead of a live region on the growing bubble itself, this
  // announces once when a response finishes — enough for a screen-reader
  // user to know it's safe to go read the reply.
  const [streamAnnouncement, setStreamAnnouncement] = useState('');
  const prevStreamingIdRef = useRef<string | null>(null);
  useEffect(() => {
    if (prevStreamingIdRef.current && !streamingId) {
      setStreamAnnouncement('Response complete.');
      const clear = setTimeout(() => setStreamAnnouncement(''), 3000);
      prevStreamingIdRef.current = streamingId;
      return () => clearTimeout(clear);
    }
    prevStreamingIdRef.current = streamingId;
  }, [streamingId]);

  // One checkbox per legacy "tool slot" (calculator, file_operations, ...) that
  // has a real MCP server configured for it — replaces the old hardcoded
  // 8-fake-tool list with whatever `mcp_servers.toml` actually defines.
  const availableTools: Tool[] = useMemo(() => {
    const bySlot = new Map<string, Tool>();
    mcpServers.forEach((s) => {
      if (s.slot) {
        bySlot.set(s.slot, {
          name: s.slot,
          description: s.connected ? s.name : `${s.name} (disconnected)`,
          enabled: false,
        });
      }
    });
    return Array.from(bySlot.values());
  }, [mcpServers]);

  // Live estimate of the conversation's token footprint, draft included, so
  // the header chip below ticks up as the user types — a heads-up before a
  // send actually trips server-side truncation, not just an after-the-fact report.
  const historyTokens = useMemo(
    () => messages.reduce((sum, m) => sum + estimateTokens(m.content), 0) + estimateTokens(input),
    [messages, input]
  );

  // Pre-compute a map for O(1) partner lookup during Compare Mode rendering,
  // avoiding O(n^2) nested find scans over the message list.
  const compareGroupBMap = useMemo(() => {
    const map = new Map<string, Message>();
    for (const m of messages) {
      if (m.compareGroupId && m.id.endsWith('-b')) {
        map.set(m.compareGroupId, m);
      }
    }
    return map;
  }, [messages]);

  useEffect(() => {
    setTools((prev) => {
      const prevEnabled = new Map(prev.map((t) => [t.name, t.enabled]));
      return availableTools.map((t) => ({ ...t, enabled: prevEnabled.get(t.name) ?? false }));
    });
  }, [availableTools]);

  useEffect(() => {
    api.listMcpServers().then((result) => {
      if (!result.error) setMcpServers(result.servers);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api]);

  useEffect(() => {
    api.getSettings().then((result) => {
      if (!result.error && result.settings?.conversation_token_limit) {
        setConversationTokenLimit(result.settings.conversation_token_limit);
      }
    });
  }, [api]);

  // chatError is store-level (survives tab unmount) but display is now the
  // shared toast queue, which owns its own auto-dismiss/close lifecycle —
  // the previous inline banner here had no way to dismiss it at all.
  useEffect(() => {
    if (error) {
      addToast({ type: 'error', message: error });
      setError(null);
    }
  }, [error, addToast, setError]);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const modelSelectorRef = useRef<HTMLDivElement>(null);
  const sessionsRef = useRef<HTMLDivElement>(null);

  const toolCallsSupported = selectedEngine?.capabilities.tool_calls ?? true;
  const structuredOutputsSupported = selectedEngine?.capabilities.structured_outputs ?? false;

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages, loading]);

  useEffect(() => {
    if (!loading) return;
    const interval = setInterval(() => setGenTick((t) => t + 1), 150);
    return () => clearInterval(interval);
  }, [loading]);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (modelSelectorRef.current && !modelSelectorRef.current.contains(event.target as Node)) {
        setShowModelSelector(false);
      }
      if (sessionsRef.current && !sessionsRef.current.contains(event.target as Node)) {
        setShowSessions(false);
      }
      if (compareSelectorRef.current && !compareSelectorRef.current.contains(event.target as Node)) {
        setShowCompareSelector(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const loadSessions = useCallback(async () => {
    setSessionsLoading(true);
    try {
      const result = await api.listSessions();
      if (!result.error && result.sessions) {
        setSessions(result.sessions);
      }
    } catch (e) {
      console.error('Failed to load sessions:', e);
    }
    setSessionsLoading(false);
  }, [api]);

  useEffect(() => {
    loadSessions();
  }, [loadSessions]);

  useEffect(() => {
    if (!toolCallsSupported) {
      setTools((prev) => prev.map((tool) => (tool.enabled ? { ...tool, enabled: false } : tool)));
    }
  }, [toolCallsSupported]);

  const handleSaveSession = async () => {
    if (messages.length === 0) return;
    const sessionId = `session_${Date.now()}`;
    const session = {
      id: sessionId,
      name: sessionName,
      model: currentModel === 'none' ? 'unknown' : currentModel,
      messages: messages,
    };
    
    const result = await api.saveSession(session);
    if (result.success) {
      loadSessions();
      setShowSessions(false);
      addToast({ type: 'success', message: `Saved "${sessionName}"` });
      setSessionName(`Session ${new Date().toLocaleDateString()}`);
    } else {
      setError(result.error || 'Failed to save session');
    }
  };

  const handleLoadSession = async (sessionId: string) => {
    if (loading) return;
    const result = await api.loadSession(sessionId);
    if (result.success && result.session) {
      const session = result.session;
      setMessages(Array.isArray(session.messages) ? session.messages : []);
      if (session.model && session.model !== 'unknown') {
        setCurrentModel(session.model);
      }
      setError(null);
      setShowSessions(false);
      addToast({ type: 'success', message: `Loaded "${session.name || session.id}"` });
    } else {
      setError(result.error || 'Failed to load session');
    }
  };

  const handleDeleteSession = async (sessionId: string) => {
    if (!window.confirm('Delete this session?')) return;
    const result = await api.deleteSession(sessionId);
    if (result.success) {
      loadSessions();
    } else {
      setError(result.error || 'Failed to delete session');
    }
  };

  const usableModels = models.filter((m) => m.status === 'Loaded' || m.status === 'Ready');

  // Builds the transcript sent to the model: existing history (`messages`,
  // read from the closure — stale by one turn since setMessages hasn't
  // flushed yet) plus the turn currently being sent, with any blank/failed
  // placeholder entries dropped so they don't waste conversation-token budget.
  const buildHistoryPayload = (latestUserText: string) =>
    [...messages, { role: 'user' as const, content: latestUserText }]
      .filter((m) => m.content.trim().length > 0)
      .map((m) => ({ role: m.role, content: m.content }));

  // The backend serves exactly one model at a time — /api/inference/chat's
  // `model` field is dead code there (it always uses whichever model was
  // last explicitly loaded via /api/models/load), so two "concurrent"
  // requests with different models would silently both hit the same loaded
  // model. A genuine comparison has to actually swap the loaded model
  // between the two halves, sequentially — there's no way to run both at
  // once without the backend gaining real multi-model serving.
  const handleCompareSend = async (messageText: string) => {
    setLoading(true);
    const groupId = `cmp-${Date.now()}`;
    const idA = `${groupId}-a`;
    const idB = `${groupId}-b`;
    const originalModel = currentModel === 'none' ? null : currentModel;
    const modelA = originalModel;
    const modelB = compareModel as string;

    setMessages((prev) => [
      ...prev,
      { role: 'assistant', content: '', id: idA, timestamp: '', model: modelA ?? 'default', compareGroupId: groupId },
      { role: 'assistant', content: '', id: idB, timestamp: '', model: modelB, compareGroupId: groupId },
    ]);
    setCompareLoadingIds(new Set([idA, idB]));

    const enabledTools = tools.filter((t) => t.enabled).map((t) => t.name);
    const useStreaming = enabledTools.length === 0;

    const setContent = (id: string, content: string) => {
      setMessages((prev) => prev.map((m) => (m.id === id ? { ...m, content } : m)));
    };

    const runOne = async (id: string, model: string | null) => {
      if (model) {
        setContent(id, 'Loading model…');
        const loadResult = await api.loadModel(model);
        if (!loadResult.success) {
          setContent(id, `[failed to load ${model}: ${loadResult.error}]`);
          setCompareLoadingIds((prev) => {
            const next = new Set(prev);
            next.delete(id);
            return next;
          });
          return;
        }
      }
      setContent(id, '');

      const result = await api.sendMessage({
        message: messageText,
        messages: buildHistoryPayload(messageText),
        temperature,
        top_p: 0.9,
        top_k: 40,
        penalty: 1.1,
        max_tokens: maxTokens,
        system_prompt: systemPrompt,
        mcp: { tools: enabledTools },
        stream: useStreaming,
        model: model ?? undefined,
      }, (token: string) => {
        setMessages((prev) => prev.map((m) => (m.id === id ? { ...m, content: m.content + token } : m)));
      });

      setCompareLoadingIds((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });

      if (result.success) {
        const data = result.data || {};
        setMessages((prev) => prev.map((m) => (m.id === id
          ? {
              ...m,
              content: useStreaming ? m.content : (data.response ?? m.content),
              truncatedBefore: !!data.truncated,
              timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
            }
          : m)));
      } else {
        setMessages((prev) => prev.map((m) => (m.id === id
          ? { ...m, content: `[error: ${result.error || 'Failed to send message'}]` }
          : m)));
      }
    };

    // Sequential, not parallel — see note above.
    await runOne(idA, modelA);
    await runOne(idB, modelB);
    setLoading(false);

    // Leave the app back on the model the user actually had selected,
    // rather than silently stranding it on whichever one ran last.
    if (originalModel) {
      api.loadModel(originalModel);
    }
  };

  const addFiles = async (files: FileList | File[]) => {
    for (const file of Array.from(files)) {
      const ext = file.name.split('.').pop()?.toLowerCase() ?? '';
      const looksTexty =
        TEXT_FILE_EXTENSIONS.has(ext) || file.type.startsWith('text/') || file.type === 'application/json';
      if (!looksTexty) {
        addToast({
          type: 'error',
          message: `${file.name}: only text-based files are supported (code, markdown, JSON, CSV, logs, config). Images and other binaries aren't sent to the model.`,
        });
        continue;
      }
      if (file.size > MAX_ATTACHMENT_BYTES) {
        addToast({
          type: 'error',
          message: `${file.name}: ${(file.size / 1024).toFixed(0)}KB exceeds the ${MAX_ATTACHMENT_BYTES / 1024}KB attachment limit.`,
        });
        continue;
      }
      try {
        const content = await file.text();
        setAttachments((prev) => [
          ...prev,
          { id: `${Date.now()}-${Math.random().toString(36).slice(2)}`, name: file.name, content, size: file.size },
        ]);
      } catch {
        addToast({ type: 'error', message: `Failed to read ${file.name}.` });
      }
    }
  };

  const handleFileInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files) addFiles(e.target.files);
    e.target.value = '';
  };

  const removeAttachment = (id: string) => setAttachments((prev) => prev.filter((a) => a.id !== id));

  const handleComposerDragOver = (e: React.DragEvent) => {
    if (!e.dataTransfer.types.includes('Files')) return;
    e.preventDefault();
    setIsDraggingFile(true);
  };
  const handleComposerDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDraggingFile(false);
  };
  const handleComposerDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDraggingFile(false);
    if (e.dataTransfer.files?.length) addFiles(e.dataTransfer.files);
  };

  const handleSend = async () => {
    // CRITICAL FIX #1: Capture input BEFORE clearing
    const attachmentsBlock = attachments
      .map((a) => `**Attached: ${a.name}**\n\`\`\`\n${a.content}\n\`\`\`\n`)
      .join('\n');
    const messageText = (attachmentsBlock ? `${attachmentsBlock}\n${input}` : input).trim();
    if (!messageText || loading) return;
    setAttachments([]);

    const userMessage: Message = {
      role: 'user',
      content: messageText,
      id: Date.now().toString(),
      timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
    };

    setMessages((prev) => [...prev, userMessage]);
    setInput('');
    setError(null);

    if (compareModel) {
      await handleCompareSend(messageText);
      return;
    }

    setLoading(true);

    const assistantId = (Date.now() + 1).toString();
    setStreamingId(assistantId);
    setMessages((prev) => [...prev, { role: 'assistant', content: '', id: assistantId, timestamp: '', model: currentModel || undefined }]);

    const enabledTools = toolCallsSupported
      ? tools.filter((t) => t.enabled).map((t) => t.name)
      : [];
    genStartRef.current = Date.now();
    genTokenCountRef.current = 0;

    // Tool calls, their real results, and any pending-confirmation card only ever
    // arrive on the plain JSON response — the token-streaming SSE path only ever
    // sends {token} chunks. So a turn with tools enabled skips streaming in
    // exchange for seeing what actually happened.
    const useStreaming = enabledTools.length === 0;

    const result = await api.sendMessage({
      message: messageText,
      messages: buildHistoryPayload(messageText),
      temperature,
      top_p: 0.9,
      top_k: 40,
      penalty: 1.1,
      max_tokens: maxTokens,
      system_prompt: systemPrompt,
      mcp: toolCallsSupported ? { tools: enabledTools } : undefined,
      stream: useStreaming,
      model: currentModel === 'none' ? undefined : currentModel,
    }, (token: string) => {
      genTokenCountRef.current += 1;
      setMessages((prev) => prev.map(m =>
        m.id === assistantId ? { ...m, content: m.content + token } : m
      ));
    });

    setLoading(false);
    setStreamingId(null);
    genStartRef.current = null;

    if (result.success) {
      const data = result.data || {};
      setMessages((prev) => prev.map(m =>
        m.id === assistantId
          ? {
              ...m,
              content: useStreaming ? m.content : (data.response ?? m.content),
              toolCalls: data.tool_results,
              pendingToolCall: data.pending_tool_call,
              truncatedBefore: !!data.truncated,
              timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
            }
          : m
      ));
    } else {
      setMessages((prev) => prev.filter(m => m.id !== assistantId));
      setError(result.error || 'Failed to send message');
    }
  };

  const handleConfirmToolCall = async (messageId: string, requestId: string, approve: boolean) => {
    setConfirmingId(requestId);
    const result = await api.confirmToolCall(requestId, approve);
    setConfirmingId(null);

    if (result.success) {
      const data = result.data || {};
      setMessages((prev) => prev.map(m =>
        m.id === messageId
          ? {
              ...m,
              content: data.response ?? m.content,
              toolCalls: data.tool_results,
              pendingToolCall: data.pending_tool_call,
              timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
            }
          : m
      ));
    } else {
      setError(result.error || 'Failed to resolve tool call');
    }
  };

  // Toggles browser speech-to-text. Rebuilds the full input value on every
  // `onresult` (base text from before recording + finalized transcript +
  // current interim chunk) rather than appending, since interim results
  // fire repeatedly for the same in-progress phrase — appending would
  // duplicate words on every partial update.
  const toggleRecording = useCallback(() => {
    const Ctor = getSpeechRecognitionCtor();
    if (!Ctor) return;
    if (isRecording) {
      recognitionRef.current?.stop();
      return;
    }
    const recognition = new Ctor();
    recognition.continuous = true;
    recognition.interimResults = true;
    recognition.lang = 'en-US';
    recordingBaseRef.current = input;
    finalTranscriptRef.current = '';

    recognition.onresult = (event: any) => {
      let interim = '';
      for (let i = event.resultIndex; i < event.results.length; i++) {
        const transcript = event.results[i][0].transcript;
        if (event.results[i].isFinal) {
          finalTranscriptRef.current += transcript;
        } else {
          interim += transcript;
        }
      }
      const combined = [recordingBaseRef.current, finalTranscriptRef.current, interim]
        .map((s) => s.trim())
        .filter(Boolean)
        .join(' ');
      setInput(combined);
    };
    recognition.onerror = () => setIsRecording(false);
    recognition.onend = () => setIsRecording(false);

    recognitionRef.current = recognition;
    recognition.start();
    setIsRecording(true);
  }, [isRecording, input]);

  // Stop any in-flight recognition if the tab unmounts mid-recording —
  // otherwise the browser keeps listening (and the mic indicator stays on
  // in the OS) with nothing left to receive the transcript.
  useEffect(() => {
    return () => {
      recognitionRef.current?.stop();
    };
  }, []);

  const handleCopy = async (content: string, id: string) => {
    try {
      await navigator.clipboard.writeText(content);
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    } catch { /* noop */ }
  };

  const handleRegenerate = () => {
    const lastUser = [...messages].reverse().find(m => m.role === 'user');
    if (lastUser) {
      setMessages((prev) => {
        const idx = prev.findIndex(m => m.role === 'assistant' && m.id === streamingId);
        return idx >= 0 ? prev.slice(0, idx) : prev;
      });
      setInput(lastUser.content);
    }
  };

  const handleDeleteMessage = (id: string) => {
    if (loading) return;
    setMessages((prev) => prev.filter((m) => m.id !== id));
  };

  // Truncates history back to (and including) the edited message, then
  // drops its text into the composer so the user can tweak and resend —
  // a linear "replace this turn onward" edit, not a branching history.
  const handleEditMessage = (msg: Message) => {
    if (loading) return;
    const idx = messages.findIndex((m) => m.id === msg.id);
    if (idx === -1) return;
    setMessages((prev) => prev.slice(0, idx));
    setInput(msg.content);
  };

  const selectModel = async (name: string) => {
    setShowModelSelector(false);
    // With Ollama, just set current model - no load/unload needed
    setCurrentModel(name);
  };

  const toggleTool = (name: string) => {
    if (!toolCallsSupported) {
      setError(`${selectedEngine?.label || 'Current engine'} does not support tool calling.`);
      return;
    }
    setTools(tools.map(t => t.name === name ? { ...t, enabled: !t.enabled } : t));
  };

  // genTick just forces a re-render on the interval; the real read happens here.
  void genTick;
  const genElapsedSec = genStartRef.current ? (Date.now() - genStartRef.current) / 1000 : 0;
  const genTokPerSec = genElapsedSec > 0 ? genTokenCountRef.current / genElapsedSec : 0;
  const streamingMsg = streamingId ? messages.find((m) => m.id === streamingId) : undefined;
  const awaitingFirstToken = loading && !!streamingMsg && streamingMsg.content === '';

  return (
    <div className="flex flex-col h-full bg-slate-950 relative">
      {/* Header / Model Selector */}
      <div className="flex items-center justify-between px-6 py-3 border-b border-slate-900 bg-slate-950/50 backdrop-blur-md sticky top-0 z-10">
        <div className="flex items-center gap-2">
            {/* eslint-disable-next-line jsx-a11y/no-static-element-interactions -- wrapper only delegates Escape from its real interactive children (button/role=option below), not a control itself */}
            <div
                className="relative"
                ref={modelSelectorRef}
                onKeyDown={(e) => { if (e.key === 'Escape') setShowModelSelector(false); }}
            >
                <button
                    onClick={() => setShowModelSelector(!showModelSelector)}
                    aria-haspopup="listbox"
                    aria-expanded={showModelSelector}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg hover:bg-slate-900 transition text-sm font-semibold group focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                >
                    <span className="text-slate-200 group-hover:text-white max-w-[200px] truncate">
                        {currentModel === 'none' ? 'Select Model' : currentModel.split('/').pop()}
                    </span>
                    <ChevronDown size={14} className={`text-slate-500 transition-transform ${showModelSelector ? 'rotate-180' : ''}`} aria-hidden="true" />
                </button>

                {showModelSelector && (
                    <div role="listbox" aria-label="Available models" className="absolute left-0 mt-2 w-72 bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl z-50 overflow-hidden animate-in fade-in zoom-in-95 duration-100">
                        <div className="p-2 max-h-[400px] overflow-y-auto">
                            {usableModels.length === 0 ? (
                                <div className="p-4 text-center text-slate-500 text-sm">No models available</div>
                            ) : (
                                usableModels.map(m => (
                                    <button
                                        key={m.name}
                                        role="option"
                                        aria-selected={currentModel === m.name}
                                        onClick={() => selectModel(m.name)}
                                        className={`flex items-center justify-between w-full px-3 py-2.5 rounded-xl text-left transition text-sm ${
                                            currentModel === m.name ? 'bg-blue-600/10 text-blue-400' : 'text-slate-300 hover:bg-slate-800'
                                        }`}
                                    >
                                        <div className="flex flex-col min-w-0">
                                            <span className="font-bold truncate">{m.name.split('/').pop()}</span>
                                            <span className="text-[10px] text-slate-500 truncate">{m.name}</span>
                                        </div>
                                        {currentModel === m.name && <Check size={14} aria-hidden="true" />}
                                    </button>
                                ))
                            )}
                        </div>
                        <div className="p-2 border-t border-slate-800 bg-slate-900/50">
                            <button
                                onClick={() => setActiveTab(1)}
                                className="flex items-center gap-2 w-full px-3 py-2 rounded-xl text-xs font-bold text-slate-400 hover:text-white hover:bg-slate-800 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                            >
                                <Plus size={14} aria-hidden="true" /> Download Models
                            </button>
                        </div>
                    </div>
                )}
            </div>

            {/* eslint-disable-next-line jsx-a11y/no-static-element-interactions -- wrapper only delegates Escape from its real interactive children, not a control itself */}
            <div
                className="relative"
                ref={compareSelectorRef}
                onKeyDown={(e) => { if (e.key === 'Escape') setShowCompareSelector(false); }}
            >
                {compareModel ? (
                    <div className="flex items-center gap-1.5 pl-2 pr-1 py-1 rounded-lg bg-purple-500/10 border border-purple-500/20 text-xs text-purple-300 font-medium">
                        <GitCompare size={12} aria-hidden="true" />
                        <span className="max-w-[120px] truncate">vs {compareModel.split('/').pop()}</span>
                        <button
                            onClick={() => setCompareModel(null)}
                            className="p-0.5 rounded hover:bg-purple-500/20 hover:text-white transition"
                            aria-label="Exit compare mode"
                        >
                            <X size={12} aria-hidden="true" />
                        </button>
                    </div>
                ) : (
                    <button
                        onClick={() => setShowCompareSelector(!showCompareSelector)}
                        aria-haspopup="listbox"
                        aria-expanded={showCompareSelector}
                        title="Compare against a second model"
                        className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-slate-300 transition text-xs font-semibold focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                    >
                        <GitCompare size={14} aria-hidden="true" />
                        Compare
                    </button>
                )}

                {showCompareSelector && (
                    <div role="listbox" aria-label="Compare against" className="absolute left-0 mt-2 w-64 bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl z-50 overflow-hidden animate-in fade-in zoom-in-95 duration-100">
                        <div className="p-2 max-h-[320px] overflow-y-auto">
                            {usableModels.filter((m) => m.name !== currentModel).length === 0 ? (
                                <div className="p-4 text-center text-slate-500 text-sm">No other models available</div>
                            ) : (
                                usableModels.filter((m) => m.name !== currentModel).map((m) => (
                                    <button
                                        key={m.name}
                                        role="option"
                                        aria-selected={false}
                                        onClick={() => { setCompareModel(m.name); setShowCompareSelector(false); }}
                                        className="flex items-center justify-between w-full px-3 py-2.5 rounded-xl text-left transition text-sm text-slate-300 hover:bg-slate-800"
                                    >
                                        <div className="flex flex-col min-w-0">
                                            <span className="font-bold truncate">{m.name.split('/').pop()}</span>
                                            <span className="text-[10px] text-slate-500 truncate">{m.name}</span>
                                        </div>
                                    </button>
                                ))
                            )}
                        </div>
                    </div>
                )}
            </div>

            {/* eslint-disable-next-line jsx-a11y/no-static-element-interactions -- wrapper only delegates Escape/arrow-key roving focus from its real interactive children, not a control itself */}
            <div
                className="relative"
                ref={sessionsRef}
                onKeyDown={(e) => {
                    if (e.key === 'Escape') { setShowSessions(false); return; }
                    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
                    const container = e.currentTarget;
                    const rows = Array.from(container.querySelectorAll<HTMLButtonElement>('button[aria-label^="Load session "]'));
                    if (rows.length === 0) return;
                    e.preventDefault();
                    const current = rows.indexOf(document.activeElement as HTMLButtonElement);
                    const next = current === -1
                        ? (e.key === 'ArrowDown' ? 0 : rows.length - 1)
                        : (e.key === 'ArrowDown' ? (current + 1) % rows.length : (current - 1 + rows.length) % rows.length);
                    rows[next].focus();
                }}
            >
              <div className="flex items-center gap-1">
                <button
                  onClick={() => { if (!loading) { setSessionName(`Session ${new Date().toLocaleDateString()}`); setShowSessions(true); }}}
                  className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  title="Save Session"
                  aria-label="Save session"
                  aria-haspopup="dialog"
                  aria-expanded={showSessions}
                >
                  <Save size={16} aria-hidden="true" />
                </button>
                <button
                  onClick={() => { loadSessions(); setShowSessions(true); }}
                  className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  title="Load Session"
                  aria-label="Load session"
                  aria-haspopup="dialog"
                  aria-expanded={showSessions}
                >
                  <FolderOpen size={16} aria-hidden="true" />
                </button>
                <button
                  onClick={() => {
                    if (loading) return;
                    if (messages.length > 0 && window.confirm('Are you sure you want to clear the current chat? This will permanently delete your active conversation.')) {
                      setMessages([]);
                    }
                  }}
                  className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  title="Clear chat"
                  aria-label="Clear chat"
                >
                    <Trash2 size={16} aria-hidden="true" />
                </button>
              </div>

              {showSessions && (
                <div role="dialog" aria-label="Sessions" className="absolute right-0 mt-2 w-80 bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl z-50 overflow-hidden animate-in fade-in zoom-in-95 duration-100">
                  <div className="p-2 border-b border-slate-800 flex items-center justify-between">
                    <h4 className="text-xs font-bold text-slate-400 uppercase tracking-wider">Sessions</h4>
                    <button onClick={() => setShowSessions(false)} className="text-slate-500 hover:text-white" aria-label="Close sessions panel"><X size={14} aria-hidden="true" /></button>
                  </div>
                  <div className="p-2 max-h-[400px] overflow-y-auto">
                    {sessionsLoading ? (
                      <div className="flex justify-center py-4">
                        <Loader size={20} className="text-blue-500 animate-spin" />
                      </div>
                    ) : sessions.length === 0 ? (
                      <div className="p-4 text-center text-slate-500 text-sm">
                        <Database size={24} className="mx-auto mb-2 text-slate-700" />
                        <p>No saved sessions</p>
                      </div>
                    ) : (
                      sessions.map((s: Session) => (
                        <div key={s.id} className="flex items-center justify-between px-3 py-2.5 rounded-xl text-left transition text-sm hover:bg-slate-800/50">
                          <div className="flex flex-col min-w-0">
                            <span className="font-bold truncate">{s.name || s.id}</span>
                            <span className="text-[10px] text-slate-500 truncate">{s.model} • {s.tokens} messages</span>
                          </div>
                          <div className="flex items-center gap-1">
                            <button
                              onClick={() => handleLoadSession(s.id)}
                              className="p-1.5 hover:bg-slate-700 rounded-lg text-slate-400 hover:text-white transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                              title="Load"
                              aria-label={`Load session ${s.name || s.id}`}
                            >
                              <FolderOpen size={12} aria-hidden="true" />
                            </button>
                            <button
                              onClick={() => handleDeleteSession(s.id)}
                              className="p-1.5 hover:bg-slate-700 rounded-lg text-slate-400 hover:text-red-400 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                              title="Delete"
                              aria-label={`Delete session ${s.name || s.id}`}
                            >
                              <Trash2 size={12} aria-hidden="true" />
                            </button>
                          </div>
                        </div>
                      ))
                    )}
                  </div>
                  <div className="p-2 border-t border-slate-800 bg-slate-900/50">
                    <div className="space-y-2">
                      <input
                        type="text"
                        value={sessionName}
                        onChange={(e) => setSessionName(e.target.value)}
                        placeholder="Session name..."
                        className="w-full bg-slate-800 border border-slate-700 rounded-xl px-3 py-2 text-sm text-slate-200 focus:outline-none focus:ring-2 focus:ring-blue-500/50 focus:border-blue-500/50"
                      />
                      <button
                        onClick={handleSaveSession}
                        disabled={!sessionName.trim() || messages.length === 0}
                        className="flex items-center gap-2 w-full px-3 py-2 rounded-xl text-xs font-bold transition disabled:opacity-50 disabled:cursor-not-allowed bg-blue-600 text-white hover:bg-blue-500 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                      >
                        <Save size={14} /> Save Current Chat
                      </button>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        <div className="flex items-center gap-2">
            {selectedEngine && (
              <div className="hidden md:flex items-center gap-2 px-2.5 py-1 rounded-lg bg-slate-900 text-[10px] font-bold uppercase tracking-wider text-slate-400 border border-slate-800">
                <span>{selectedEngine.label}</span>
                {!toolCallsSupported && <span className="text-orange-400">No tool calls</span>}
                {structuredOutputsSupported && <span className="text-emerald-400">Structured outputs</span>}
              </div>
            )}
            {(() => {
                const ratio = conversationTokenLimit > 0 ? historyTokens / conversationTokenLimit : 0;
                const tone = ratio >= 0.9
                    ? 'bg-red-500/10 border-red-500/30 text-red-400'
                    : ratio >= 0.7
                    ? 'bg-amber-500/10 border-amber-500/30 text-amber-400'
                    : 'bg-slate-900/60 border-slate-800 text-slate-500';
                const dotTone = ratio >= 0.9 ? 'bg-red-400 animate-pulse' : ratio >= 0.7 ? 'bg-amber-400' : 'bg-slate-600';
                const fmt = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n));
                return (
                    <div
                        className={`flex items-center gap-1.5 px-2.5 py-1 rounded-full border text-[11px] font-mono ${tone}`}
                        title={`~${historyTokens} of ${conversationTokenLimit} conversation-memory tokens used. Oldest turns are dropped first once this fills up — adjust in Settings → Conversation Token Limit.`}
                    >
                        <span className={`w-1.5 h-1.5 rounded-full ${dotTone}`} aria-hidden="true" />
                        {fmt(historyTokens)}/{fmt(conversationTokenLimit)}
                    </div>
                );
            })()}
            <div className="w-8 h-8 rounded-full bg-orange-500 flex items-center justify-center text-xs font-bold text-white shadow-lg shadow-orange-500/20" aria-hidden="true">R</div>
        </div>
      </div>

      {/* Message Area */}
      <div className="sr-only" role="status" aria-live="polite">{streamAnnouncement}</div>
      <div className="flex-1 overflow-y-auto px-4 py-8 space-y-8" tabIndex={0} role="region" aria-label="Chat">
        <div className="max-w-3xl mx-auto space-y-10" role="log" aria-label="Conversation" aria-live="polite" aria-relevant="additions">
            {messages.length === 0 && (
                <div className="flex flex-col items-center justify-center h-[50vh] text-center space-y-4 animate-in fade-in duration-500">
                    <div className="w-16 h-16 bg-slate-900 rounded-2xl flex items-center justify-center mb-2 shadow-2xl border border-slate-800">
                        <Bot size={32} className="text-blue-500" />
                    </div>
                    <h2 className="text-2xl font-bold text-white">How can I help you today?</h2>
                    <p className="text-slate-500 max-w-sm mb-4">Ask me to execute build pipelines, audit cryptography, or validate network topologies.</p>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-3 max-w-xl w-full px-4">
                        {SUGGESTIONS.map((s) => (
                            <button
                                key={s.text}
                                onClick={() => {
                                    setInput(s.text);
                                    textareaRef.current?.focus();
                                }}
                                aria-label={s.label}
                                className="p-3.5 text-left bg-slate-900/50 hover:bg-slate-800/80 border border-slate-800 hover:border-slate-700 text-xs text-slate-300 hover:text-white rounded-xl transition duration-200 active:scale-[0.98] focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                            >
                                {s.text}
                            </button>
                        ))}
                    </div>
                </div>
            )}

            {messages.map((msg) => {
                // The "-b" half of a compare pair renders as part of its "-a"
                // partner below, not on its own.
                if (msg.compareGroupId && msg.id.endsWith('-b')) return null;
                if (msg.compareGroupId && msg.id.endsWith('-a')) {
                    const partner = compareGroupBMap.get(msg.compareGroupId);
                    return (
                        <CompareRow
                            key={msg.compareGroupId}
                            a={msg}
                            b={partner}
                            compareLoadingIds={compareLoadingIds}
                            copiedId={copiedId}
                            onCopy={handleCopy}
                        />
                    );
                }
                return (
                <React.Fragment key={msg.id}>
                {msg.truncatedBefore && (
                    <div role="separator" aria-label="Earlier messages were trimmed to stay within the conversation token limit" className="flex items-center gap-3 py-1">
                        <div className="flex-1 h-px bg-slate-800" />
                        <span className="text-[10px] text-slate-600 whitespace-nowrap">earlier messages trimmed to fit memory</span>
                        <div className="flex-1 h-px bg-slate-800" />
                    </div>
                )}
                <div className={`flex gap-4 ${msg.role === 'user' ? 'justify-end' : 'justify-start'} group animate-in fade-in slide-in-from-bottom-2 duration-300`}>
                    {msg.role === 'assistant' && (
                        <div className="flex-shrink-0 w-8 h-8 rounded-lg bg-slate-900 border border-slate-800 flex items-center justify-center mt-1">
                            <div className="w-5 h-5 bg-blue-600 rounded-sm flex items-center justify-center text-[10px] font-bold text-white uppercase">
                                {msg.model ? msg.model.substring(0, 1) : 'G'}
                            </div>
                        </div>
                    )}

                    <div className={`max-w-[85%] flex flex-col gap-2 ${msg.role === 'user' ? 'items-end' : 'items-start'}`}>
                        <div className="flex items-center gap-2 px-1">
                            <span className="text-xs font-bold text-slate-200">
                                {msg.role === 'user' ? 'You' : (msg.model ? msg.model.split('/').pop() : 'Ghostlink')}
                            </span>
                            <span className="text-[10px] text-slate-500">{msg.timestamp}</span>
                        </div>

                        {msg.toolCalls && msg.toolCalls.length > 0 && (
                            <div className="flex flex-col gap-1 px-1 w-full">
                                {msg.toolCalls.map((call, i) => (
                                    <div
                                        key={i}
                                        className={`flex items-start gap-2 px-3 py-1.5 rounded-xl text-[11px] font-mono border ${
                                            call.success
                                                ? 'bg-slate-900/50 border-slate-800 text-slate-400'
                                                : 'bg-red-950/30 border-red-900/50 text-red-400'
                                        }`}
                                    >
                                        <Wrench size={12} className="mt-0.5 shrink-0" />
                                        <span className="truncate">
                                            <span className="font-bold">{call.tool}</span>
                                            {call.server && <span className="text-slate-600"> @ {call.server}</span>}
                                            {' → '}
                                            <span className="break-all">{call.result}</span>
                                        </span>
                                    </div>
                                ))}
                            </div>
                        )}

                        {msg.pendingToolCall && (
                            <div
                                role="alert"
                                aria-labelledby={`tool-approval-heading-${msg.id}`}
                                className="flex flex-col gap-2 px-4 py-3 rounded-2xl border border-amber-500/30 bg-amber-500/10 w-full max-w-md"
                            >
                                <div id={`tool-approval-heading-${msg.id}`} className="flex items-center gap-2 text-amber-400 text-xs font-bold">
                                    <ShieldAlert size={14} aria-hidden="true" />
                                    Approval needed
                                </div>
                                <p className="text-xs text-slate-300">
                                    Run <span className="font-mono font-bold">{msg.pendingToolCall.tool}</span> on server{' '}
                                    <span className="font-mono font-bold">{msg.pendingToolCall.server}</span>?
                                </p>
                                <pre className="text-[10px] text-slate-500 bg-slate-950/50 rounded-lg p-2 overflow-x-auto">
                                    {JSON.stringify(msg.pendingToolCall.args, null, 2)}
                                </pre>
                                <div className="flex gap-2">
                                    <button
                                        // eslint-disable-next-line jsx-a11y/no-autofocus -- this is an alert demanding an immediate approve/deny decision; moving focus here matches WAI-ARIA alertdialog guidance
                                        autoFocus
                                        onClick={() => handleConfirmToolCall(msg.id, msg.pendingToolCall!.request_id, true)}
                                        disabled={confirmingId === msg.pendingToolCall.request_id}
                                        className="flex-1 px-3 py-1.5 bg-green-600 hover:bg-green-500 disabled:opacity-50 text-white text-xs font-bold rounded-lg transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                                    >
                                        Approve
                                    </button>
                                    <button
                                        onClick={() => handleConfirmToolCall(msg.id, msg.pendingToolCall!.request_id, false)}
                                        disabled={confirmingId === msg.pendingToolCall.request_id}
                                        className="flex-1 px-3 py-1.5 bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-300 text-xs font-bold rounded-lg transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                                    >
                                        Deny
                                    </button>
                                </div>
                            </div>
                        )}

                        <div className={`px-4 py-3 rounded-2xl text-sm leading-relaxed ${
                            msg.role === 'user'
                                ? 'bg-slate-800 text-slate-100 rounded-tr-none'
                                : 'text-slate-200 bg-transparent'
                        }`}>
                            {msg.role === 'assistant' ? (
                              <div className={`prose prose-invert ${MARKDOWN_PROSE_CLASSES}`}>
                                <ReactMarkdown
                                  remarkPlugins={[remarkGfm]}
                                  rehypePlugins={[rehypeHighlight]}
                                  components={{ pre: CodeBlock, table: MarkdownTable }}
                                >
                                  {msg.content}
                                </ReactMarkdown>
                                {msg.id === streamingId && msg.content !== '' && (
                                  <span className="inline-block w-[7px] h-[1em] bg-blue-400 align-text-bottom ml-0.5 animate-pulse" />
                                )}
                              </div>
                            ) : (
                              <div className="whitespace-pre-wrap break-words">{msg.content}</div>
                            )}
                        </div>

                        {msg.id === streamingId && msg.content !== '' && loading && (
                            <div className="px-1 -mt-1 text-[10px] text-slate-600 font-mono">
                                Generating · {genElapsedSec.toFixed(1)}s · {genTokPerSec.toFixed(1)} tok/s
                            </div>
                        )}

                        {msg.role === 'assistant' && (
                            <div className="flex items-center gap-1 mt-1 opacity-0 group-hover:opacity-100 transition-opacity focus-within:opacity-100">
                                <button onClick={() => handleCopy(msg.content, msg.id)} className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-slate-300 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none" title="Copy" aria-label="Copy response">
                                  {copiedId === msg.id ? <Check size={14} className="text-green-400" aria-hidden="true" /> : <Copy size={14} aria-hidden="true" />}
                                </button>
                                <button className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-slate-300 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none" title="Good response" aria-label="Good response">
                                    <ThumbsUp size={14} aria-hidden="true" />
                                </button>
                                <button className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-slate-300 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none" title="Bad response" aria-label="Bad response">
                                    <ThumbsDown size={14} aria-hidden="true" />
                                </button>
                                <button onClick={handleRegenerate} className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-slate-300 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none" title="Regenerate" aria-label="Regenerate response">
                                    <RotateCcw size={14} aria-hidden="true" />
                                </button>
                                <button
                                  onClick={() => { try { speechSynthesis.speak(new SpeechSynthesisUtterance(msg.content)); } catch {} }}
                                  className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-slate-300 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                                  title="Read aloud"
                                  aria-label="Read response aloud"
                                >
                                    <Volume2 size={14} aria-hidden="true" />
                                </button>
                                <button
                                  onClick={() => handleDeleteMessage(msg.id)}
                                  disabled={loading}
                                  className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-red-400 transition disabled:opacity-40 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                                  title="Delete"
                                  aria-label="Delete response"
                                >
                                    <Trash2 size={14} aria-hidden="true" />
                                </button>
                            </div>
                        )}

                        {msg.role === 'user' && (
                            <div className="flex items-center gap-1 mt-1 opacity-0 group-hover:opacity-100 transition-opacity focus-within:opacity-100">
                                <button
                                  onClick={() => handleEditMessage(msg)}
                                  disabled={loading}
                                  className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-slate-300 transition disabled:opacity-40 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                                  title="Edit and resend"
                                  aria-label="Edit and resend message"
                                >
                                    <Pencil size={14} aria-hidden="true" />
                                </button>
                                <button
                                  onClick={() => handleDeleteMessage(msg.id)}
                                  disabled={loading}
                                  className="p-1.5 rounded-lg hover:bg-slate-900 text-slate-500 hover:text-red-400 transition disabled:opacity-40 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                                  title="Delete"
                                  aria-label="Delete message"
                                >
                                    <Trash2 size={14} aria-hidden="true" />
                                </button>
                            </div>
                        )}
                    </div>

                    {msg.role === 'user' && (
                        <div className="flex-shrink-0 w-8 h-8 rounded-full bg-slate-800 border border-slate-700 flex items-center justify-center mt-1">
                            <User size={16} className="text-slate-400" />
                        </div>
                    )}
                </div>
                </React.Fragment>
                );
            })}

            {awaitingFirstToken && (
                <div className="flex gap-4 justify-start" role="status" aria-live="polite">
                    <div className="flex-shrink-0 w-8 h-8 rounded-lg bg-slate-900 border border-slate-800 flex items-center justify-center mt-1">
                        <Loader size={14} className="text-blue-500 animate-spin" aria-hidden="true" />
                    </div>
                    <div className="flex flex-col gap-1.5 justify-center">
                        <div className="flex items-center gap-1.5 text-xs text-slate-400">
                            <span className="flex gap-0.5" aria-hidden="true">
                                <span className="w-1.5 h-1.5 rounded-full bg-blue-500 animate-bounce [animation-delay:-0.3s]" />
                                <span className="w-1.5 h-1.5 rounded-full bg-blue-500 animate-bounce [animation-delay:-0.15s]" />
                                <span className="w-1.5 h-1.5 rounded-full bg-blue-500 animate-bounce" />
                            </span>
                            <span>
                                {currentModel && currentModel !== 'none' ? `${currentModel.split('/').pop()} is thinking` : 'Thinking'}
                                {' · '}{genElapsedSec.toFixed(1)}s
                            </span>
                        </div>
                    </div>
                </div>
            )}

            <div ref={messagesEndRef} />
        </div>
      </div>

      {/* Input Area */}
      <div className="px-4 pb-6 pt-2">
        <div className="max-w-3xl mx-auto relative group">
          {/* Active Tools Indicators */}
          <div className="flex flex-wrap gap-2 mb-2 px-1">
              {tools.filter(t => t.enabled).map(t => (
                  <div key={t.name} className="flex items-center gap-1.5 px-2 py-1 rounded-full bg-purple-500/10 border border-purple-500/20 text-[10px] text-purple-400 font-medium">
                      <Wand2 size={10} aria-hidden="true" />
                      {t.name}
                      <button onClick={() => toggleTool(t.name)} className="hover:text-white" aria-label={`Disable ${t.name} tool`}><X size={10} aria-hidden="true" /></button>
                  </div>
              ))}
          </div>

          {!toolCallsSupported && (
            <div className="mb-2 px-3 py-2 rounded-2xl bg-orange-500/10 border border-orange-500/20 text-xs text-orange-300">
              {selectedEngine?.label || 'Current engine'} does not support tool calling. Tool selections are disabled for this chat engine.
            </div>
          )}

          {attachments.length > 0 && (
            <div className="flex flex-wrap gap-2 mb-2 px-1">
              {attachments.map((a) => (
                <div key={a.id} className="flex items-center gap-1.5 px-2 py-1 rounded-full bg-blue-500/10 border border-blue-500/20 text-[10px] text-blue-300 font-medium">
                  <FileText size={10} aria-hidden="true" />
                  {a.name}
                  <span className="text-blue-400/60">{(a.size / 1024).toFixed(0)}KB</span>
                  <button onClick={() => removeAttachment(a.id)} className="hover:text-white" aria-label={`Remove attachment ${a.name}`}>
                    <X size={10} aria-hidden="true" />
                  </button>
                </div>
              ))}
            </div>
          )}

          <input
            ref={fileInputRef}
            type="file"
            multiple
            onChange={handleFileInputChange}
            className="sr-only"
            aria-label="Attach files to message"
          />

          {/* eslint-disable-next-line jsx-a11y/no-static-element-interactions -- wrapper only delegates Escape from the real interactive textarea/buttons inside, not a control itself */}
          <div
            className={`relative bg-slate-900 border rounded-3xl p-2 shadow-2xl focus-within:border-blue-500 focus-within:ring-2 focus-within:ring-blue-500/50 transition-all duration-300 ${
              isDraggingFile ? 'border-blue-500 ring-2 ring-blue-500/50' : 'border-slate-800'
            }`}
            onKeyDown={(e) => { if (e.key === 'Escape') setShowTools(false); }}
            onDragOver={handleComposerDragOver}
            onDragLeave={handleComposerDragLeave}
            onDrop={handleComposerDrop}
          >
            <textarea
              ref={textareaRef}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                      e.preventDefault();
                      handleSend();
                  } else if (e.key === 'Escape') {
                      (e.target as HTMLTextAreaElement).blur();
                  }
              }}
              placeholder="Send a Message"
              aria-label="Chat message"
              className="w-full bg-transparent border-none text-slate-100 placeholder-slate-500 focus:ring-0 resize-none px-4 pt-3 pb-12 text-sm min-h-[56px] max-h-48"
              rows={1}
            />

            <div className="absolute left-3 bottom-3 flex items-center gap-1">
                <button
                    onClick={() => setShowTools(!showTools)}
                    disabled={!toolCallsSupported}
                    title={toolCallsSupported ? 'Select tools' : 'Tool calling is unavailable for this engine'}
                    aria-haspopup="menu"
                    aria-expanded={showTools}
                    aria-label="Tools"
                    className={`p-2 rounded-xl transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${showTools ? 'bg-slate-800 text-blue-400' : 'text-slate-500 hover:text-slate-300 hover:bg-slate-800'} ${!toolCallsSupported ? 'opacity-40 cursor-not-allowed hover:bg-transparent hover:text-slate-500' : ''}`}
                >
                    <Plus size={20} aria-hidden="true" />
                </button>
                <button
                    onClick={() => fileInputRef.current?.click()}
                    title="Attach text files (code, markdown, JSON, CSV, logs, config)"
                    aria-label="Attach files"
                    className="p-2 rounded-xl transition text-slate-500 hover:text-slate-300 hover:bg-slate-800 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                >
                    <Paperclip size={18} aria-hidden="true" />
                </button>
            </div>

            <div className="absolute right-3 bottom-3 flex items-center gap-1">
                {getSpeechRecognitionCtor() && (
                    <button
                        onClick={toggleRecording}
                        aria-label={isRecording ? 'Stop voice input' : 'Start voice input'}
                        aria-pressed={isRecording}
                        title={isRecording ? 'Stop voice input' : 'Voice input (requires internet — browser speech recognition)'}
                        className={`relative p-2 rounded-full transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
                            isRecording
                                ? 'bg-red-500/20 text-red-400'
                                : 'text-slate-500 hover:text-slate-300 hover:bg-slate-800'
                        }`}
                    >
                        <Mic size={18} aria-hidden="true" />
                        {isRecording && (
                            <span className="absolute inset-0 rounded-full bg-red-500/30 animate-ping" aria-hidden="true" />
                        )}
                    </button>
                )}
                <button
                    onClick={handleSend}
                    disabled={(!input.trim() && attachments.length === 0) || loading}
                    aria-label="Send message"
                    className={`p-2 rounded-full transition shadow-lg focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
                        (!input.trim() && attachments.length === 0) || loading
                            ? 'bg-slate-800 text-slate-600'
                            : 'bg-white text-black hover:bg-slate-200'
                    }`}
                >
                    {loading ? <Loader size={18} className="animate-spin" aria-hidden="true" /> : <Send size={18} fill="currentColor" aria-hidden="true" />}
                </button>
            </div>

            {/* Tool Selection Popup */}
            {showTools && (
                <div role="menu" aria-label="Tools" className="absolute left-0 bottom-16 w-64 bg-slate-900 border border-slate-800 rounded-2xl p-4 shadow-2xl z-20 animate-in fade-in slide-in-from-bottom-2">
                    <div className="flex items-center justify-between mb-3 px-1">
                        <h4 className="text-xs font-bold text-slate-400 uppercase tracking-wider">Tools</h4>
                        <button onClick={() => setShowTools(false)} className="text-slate-500 hover:text-white focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none" aria-label="Close tools menu"><X size={14} aria-hidden="true" /></button>
                    </div>
                    <div className="grid grid-cols-1 gap-1">
                        {tools.map(tool => (
                            <button
                                key={tool.name}
                                role="menuitemcheckbox"
                                aria-checked={tool.enabled}
                                onClick={() => toggleTool(tool.name)}
                                className={`flex items-center justify-between w-full px-3 py-2 rounded-xl transition text-xs focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
                                    tool.enabled ? 'bg-blue-600/20 text-blue-400' : 'text-slate-400 hover:bg-slate-800'
                                }`}
                            >
                                <div className="flex items-center gap-2">
                                    <Wand2 size={14} className={tool.enabled ? 'text-blue-400' : 'text-slate-600'} aria-hidden="true" />
                                    <span>{toolDisplayName(tool.name)}</span>
                                </div>
                                {tool.enabled && <div className="w-1.5 h-1.5 bg-blue-400 rounded-full shadow-[0_0_8px_rgba(96,165,250,0.5)]" aria-hidden="true"></div>}
                            </button>
                        ))}
                    </div>
                </div>
            )}
          </div>

          <div className="mt-2 text-center">
              <p className="text-[10px] text-slate-600">Ghostlink can make mistakes. Check important info.</p>
          </div>
        </div>
      </div>
    </div>
  );
};
