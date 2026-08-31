import React, { useState, useEffect, useMemo, useRef, useCallback } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import {
  Send,
  Loader,
  Plus,
  ChevronDown,
  X,
  User,
  Bot,
  Copy,
  ThumbsUp,
  ThumbsDown,
  RotateCcw,
  Check,
  Wrench,
  Trash2,
  Pencil,
  Mic,
  Paperclip,
  FileText,
  Search,
  Pin,
  PinOff,
  Square,
  BookOpen,
  SlidersHorizontal,
  Sparkles,
  MessageSquare,
  PanelLeftClose,
  PanelLeft,
} from "lucide-react";
import {
  useAppStore,
  ChatMessage,
  Thread,
  } from "../store";
import { GhostlinkAPI } from "../api";
import { useInferenceEngines } from "../hooks/useInferenceEngines";

type Message = ChatMessage;

function getSpeechRecognitionCtor(): any {
  if (typeof window === "undefined") return undefined;
  return (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
}


function estimateTokens(text: string): number {
  if (!text) return 0;
  return Math.ceil(text.length / 4);
}

function getNodeText(node: React.ReactNode): string {
  if (node == null) return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(getNodeText).join("");
  if (typeof node === "object" && "props" in node) {
    return getNodeText((node as React.ReactElement).props.children);
  }
  return "";
}

const CodeBlock: React.FC<{ children?: React.ReactNode }> = ({ children, ...rest }) => {
  const codeText = useMemo(() => getNodeText(children).replace(/\n$/, ""), [children]);
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(codeText);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* noop */
    }
  };

  return (
    <div className="relative group my-3 rounded-xl overflow-hidden border border-slate-800 bg-slate-900/90 shadow-lg">
      <div className="flex items-center justify-between px-4 py-1.5 bg-slate-950/80 border-b border-slate-800/80 text-xs text-slate-400 font-mono">
        <span className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider">Code</span>
        <button
          onClick={handleCopy}
          className="flex items-center gap-1.5 px-2 py-1 rounded-md text-slate-400 hover:text-white hover:bg-slate-800 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          aria-label={copied ? "Copied code" : "Copy code"}
          title={copied ? "Copied!" : "Copy code"}
        >
          {copied ? <Check size={13} className="text-green-400" /> : <Copy size={13} />}
          <span className="text-[11px] font-medium">{copied ? "Copied!" : "Copy"}</span>
        </button>
      </div>
      <pre className="p-4 overflow-x-auto text-xs leading-relaxed text-slate-200 font-mono" {...rest}>
        {children}
      </pre>
    </div>
  );
};

const MarkdownTable: React.FC<React.TableHTMLAttributes<HTMLTableElement>> = (props) => (
  <div className="my-3 overflow-x-auto rounded-xl border border-slate-800 shadow-md">
    <table className="w-full text-xs text-left text-slate-300 border-collapse" {...props} />
  </div>
);

const MARKDOWN_PROSE_CLASSES =
  "prose prose-invert max-w-none prose-p:leading-relaxed prose-p:my-1.5 prose-pre:p-0 prose-pre:m-0 prose-pre:bg-transparent prose-code:text-blue-300 prose-code:bg-slate-800/50 prose-code:px-1.5 prose-code:py-0.5 prose-code:rounded prose-code:before:content-none prose-code:after:content-none prose-headings:text-slate-100 prose-headings:font-bold prose-headings:my-2 prose-ul:my-1.5 prose-ol:my-1.5 prose-li:my-0.5 prose-strong:text-slate-100 prose-a:text-blue-400 hover:prose-a:underline prose-th:bg-slate-900 prose-th:p-2 prose-th:border prose-th:border-slate-800 prose-td:p-2 prose-td:border prose-td:border-slate-800";

const MAX_ATTACHMENT_BYTES = 256 * 1024;
const TEXT_FILE_EXTENSIONS = new Set([
  "txt", "md", "json", "js", "ts", "tsx", "jsx", "py", "rs", "go", "c", "cpp", "h", "hpp", "java",
  "yaml", "yml", "toml", "ini", "log", "sh", "bash", "ps1", "sql", "css", "scss", "html", "htm", "xml", "svg",
]);

const SUGGESTIONS = [
  { text: "Check active cluster node health", label: "Ask about active cluster node health" },
  { text: "Detail GGUF local model execution", label: "Ask about GGUF local model execution" },
  { text: "List all available capabilities", label: "Ask about available capabilities" },
  { text: "Show performance metrics overview", label: "Ask about performance metrics overview" },
];

export const ChatTab: React.FC<{ api: GhostlinkAPI }> = ({ api }) => {
  const {
    currentModel, models, setCurrentModel, mcpServers, setMcpServers,
    chatMessages: messages, setChatMessages: setMessages,
    chatLoading: loading, setChatLoading: setLoading,
    chatStreamingId: streamingId, setChatStreamingId: setStreamingId,
    chatError: error, setChatError: setError,
    threads, activeThreadId, selectThread, createThread, renameThread, deleteThread, togglePinThread,
    updateActiveThread, presets, userPrompts,
    addToast, setActiveTab, metrics,
  } = useAppStore();

  const activeThread = useMemo(() => threads.find((t) => t.id === activeThreadId), [threads, activeThreadId]);
  const [placementPlan, setPlacementPlan] = useState<any>(null);

  const [input, setInput] = useState("");
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [messageRatings, setMessageRatings] = useState<Record<string, "up" | "down">>({});
  const [isRecording, setIsRecording] = useState(false);
  const recognitionRef = useRef<any>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const abortControllerRef = useRef<AbortController | null>(null);

  // Sidebar & overlay UI state
  const [showSidebar, setShowSidebar] = useState(true);
  const [threadSearch, setThreadSearch] = useState("");
  const [renamingThreadId, setRenamingThreadId] = useState<string | null>(null);
  const [renameTitleInput, setRenameTitleInput] = useState("");

  // Knobs & presets
  const [showKnobs, setShowKnobs] = useState(false);
  const [showPromptLibrary, setShowPromptLibrary] = useState(false);
  const [selectedPresetId, setSelectedPresetId] = useState("default");

  // Thread specific settings or fallbacks
  const threadTemperature = activeThread?.temperature ?? 0.7;
  const threadMaxTokens = activeThread?.maxTokens ?? 2048;
  const threadTopP = activeThread?.top_p ?? 0.9;
  const threadPenalty = activeThread?.penalty ?? 1.1;
  const threadSystemPrompt = activeThread?.systemPrompt ?? (presets.find((p) => p.id === selectedPresetId)?.prompt || "You are a production-grade, kernel-bypass-aware system orchestration co-pilot.");

  // Editing state for user turns
  const [editingMsgId, setEditingMsgId] = useState<string | null>(null);
  const [editingMsgText, setEditingMsgText] = useState("");

  const recordingBaseRef = useRef("");
  const finalTranscriptRef = useRef("");

  const [conversationTokenLimit, setConversationTokenLimit] = useState(3072);
  const [showModelSelector, setShowModelSelector] = useState(false);
  const [tools, setTools] = useState<{ name: string; description: string; enabled: boolean }[]>([]);
  const { selectedEngine } = useInferenceEngines(api);

  const [attachments, setAttachments] = useState<{ id: string; name: string; content: string; size: number }[]>([]);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [isDraggingFile, setIsDraggingFile] = useState(false);

  // Generation stats
  const genStartRef = useRef<number | null>(null);
  const genTokenCountRef = useRef(0);
  const [genTick, setGenTick] = useState(0);

  const [streamAnnouncement, setStreamAnnouncement] = useState("");
  const prevStreamingIdRef = useRef<string | null>(null);

  useEffect(() => {
    if (prevStreamingIdRef.current && !streamingId) {
      setStreamAnnouncement("Response complete.");
      const clear = setTimeout(() => setStreamAnnouncement(""), 3000);
      prevStreamingIdRef.current = streamingId;
      return () => clearTimeout(clear);
    }
    prevStreamingIdRef.current = streamingId;
  }, [streamingId]);

  const availableTools = useMemo(() => {
    const bySlot = new Map<string, { name: string; description: string; enabled: boolean }>();
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

  const historyTokens = useMemo(
    () => messages.reduce((sum, m) => sum + estimateTokens(m.content), 0) + estimateTokens(input),
    [messages, input]
  );

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
    if (api.getClusterTopology) {
      api.getClusterTopology().then((res: any) => {
        if (!res.error && res.topology?.placement_plan) {
          setPlacementPlan(res.topology.placement_plan);
        }
      });
    }
  }, [api]);

  useEffect(() => {
    api.getSettings().then((result) => {
      if (!result.error && result.settings?.conversation_token_limit) {
        setConversationTokenLimit(result.settings.conversation_token_limit);
      }
    });
  }, [api]);

  useEffect(() => {
    if (error) {
      addToast({ type: "error", message: error });
      setError(null);
    }
  }, [error, addToast, setError]);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const modelSelectorRef = useRef<HTMLDivElement>(null);

  const toolCallsSupported = selectedEngine?.capabilities.tool_calls ?? true;

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
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
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  // Keyboard shortcut Ctrl+Shift+O for New Chat
  useEffect(() => {
    const handleGlobalKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === "o") {
        e.preventDefault();
        createThread();
        addToast({ type: "info", message: "New chat started" });
      }
    };
    window.addEventListener("keydown", handleGlobalKey);
    return () => window.removeEventListener("keydown", handleGlobalKey);
  }, [createThread, addToast]);

  const usableModels = models.filter((m) => m.status === "Loaded" || m.status === "Ready");

  const buildHistoryPayload = (latestUserText: string, currentMsgs: Message[]) =>
    [...currentMsgs, { role: "user" as const, content: latestUserText }]
      .filter((m) => m.content.trim().length > 0 && !("isDivider" in m && (m as any).isDivider))
      .map((m) => ({ role: m.role, content: m.content }));

  const handleStop = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
    if (activeThreadId) {
      api.cancelSession(activeThreadId);
    }
    setLoading(false);
    setStreamingId(null);
    genStartRef.current = null;
    addToast({ type: "info", message: "Inference stopped" });
  }, [activeThreadId, api, setLoading, setStreamingId, addToast]);

  const handleSendWithMessages = async (messageText: string, baseMessages: Message[]) => {
    if (!messageText.trim() || loading) return;

    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    const controller = new AbortController();
    abortControllerRef.current = controller;

    setLoading(true);
    const assistantId = (Date.now() + 1).toString();
    setStreamingId(assistantId);

    const updatedMsgs: Message[] = [
      ...baseMessages,
      {
        role: "assistant",
        content: "",
        id: assistantId,
        timestamp: "",
        model: currentModel === "none" ? undefined : currentModel,
      },
    ];
    setMessages(updatedMsgs);

    const enabledTools = toolCallsSupported
      ? tools.filter((t) => t.enabled).map((t) => t.name)
      : [];
    genStartRef.current = Date.now();
    genTokenCountRef.current = 0;

    const useStreaming = enabledTools.length === 0;

    const result = await api.sendMessage(
      {
        message: messageText,
        messages: buildHistoryPayload(messageText, baseMessages),
        temperature: threadTemperature,
        top_p: threadTopP,
        top_k: 40,
        penalty: threadPenalty,
        max_tokens: threadMaxTokens,
        system_prompt: threadSystemPrompt,
        mcp: toolCallsSupported ? { tools: enabledTools } : undefined,
        stream: useStreaming,
        model: currentModel === "none" ? undefined : currentModel,
        signal: controller.signal,
      },
      (token: string) => {
        genTokenCountRef.current += 1;
        setMessages((prev) =>
          prev.map((m) => (m.id === assistantId ? { ...m, content: m.content + token } : m))
        );
      }
    );

    setLoading(false);
    setStreamingId(null);
    genStartRef.current = null;
    abortControllerRef.current = null;

    if (result.success) {
      const data = result.data || {};
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantId
            ? {
                ...m,
                content: useStreaming ? m.content : (data.response ?? m.content),
                toolCalls: data.tool_results,
                pendingToolCall: data.pending_tool_call,
                truncatedBefore: !!data.truncated,
                timestamp: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
              }
            : m
        )
      );
    } else if (result.error !== "The user aborted a request.") {
      setMessages((prev) => prev.filter((m) => m.id !== assistantId));
      setError(result.error || "Failed to send message");
    }
  };

  const handleSend = async () => {
    const attachmentsBlock = attachments
      .map((a) => `**Attached: ${a.name}**\n\`\`\`\n${a.content}\n\`\`\`\n`)
      .join("\n");
    const messageText = (attachmentsBlock ? `${attachmentsBlock}\n${input}` : input).trim();
    if (!messageText || loading) return;

    setAttachments([]);

    const userMessage: Message = {
      role: "user",
      content: messageText,
      id: Date.now().toString(),
      timestamp: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
    };

    const nextBase = [...messages, userMessage];
    setInput("");
    setError(null);

    await handleSendWithMessages(messageText, nextBase.slice(0, -1).concat(userMessage));
  };

  // User Turn Edit & Truncate
  const handleSaveUserEdit = async (msgId: string) => {
    if (!editingMsgText.trim() || loading) return;
    const msgIdx = messages.findIndex((m) => m.id === msgId);
    if (msgIdx === -1) return;

    const truncated = messages.slice(0, msgIdx);
    const editedUserMsg: Message = {
      ...messages[msgIdx],
      content: editingMsgText.trim(),
      timestamp: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
    };

    setEditingMsgId(null);
    setEditingMsgText("");

    const baseMsgs = [...truncated, editedUserMsg];
    await handleSendWithMessages(editedUserMsg.content, baseMsgs);
  };

  // Assistant Turn Regenerate
  const handleRegenerateAssistant = async (assistantMsgId: string) => {
    if (loading) return;
    const msgIdx = messages.findIndex((m) => m.id === assistantMsgId);
    if (msgIdx === -1) return;

    // Find preceding user turn
    let precedingUserIdx = -1;
    for (let i = msgIdx - 1; i >= 0; i--) {
      if (messages[i].role === "user") {
        precedingUserIdx = i;
        break;
      }
    }
    if (precedingUserIdx === -1) return;

    const baseMsgs = messages.slice(0, precedingUserIdx + 1);
    const lastUserText = messages[precedingUserIdx].content;

    await handleSendWithMessages(lastUserText, baseMsgs);
  };

  // Model selection mid-thread with divider
  const selectModel = async (name: string) => {
    setShowModelSelector(false);
    if (name === currentModel) return;

    setCurrentModel(name);
    if (activeThreadId) {
      updateActiveThread((t) => ({ ...t, model: name }));
    }

    if (messages.length > 0) {
      const dividerMsg: Message = {
        id: `divider_${Date.now()}`,
        role: "assistant",
        content: `Switched model to ${name}`,
        timestamp: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }),
        isDivider: true,
      };
      setMessages([...messages, dividerMsg]);
    }
  };

  const handleCopy = async (content: string, id: string) => {
    try {
      await navigator.clipboard.writeText(content);
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    } catch {
      /* noop */
    }
  };

  const handleRating = (id: string, type: "up" | "down") => {
    setMessageRatings((prev) => {
      const current = prev[id];
      if (current === type) {
        const next = { ...prev };
        delete next[id];
        addToast({ type: "info", message: "Rating removed" });
        return next;
      }
      addToast({
        type: "success",
        message: type === "up" ? "Feedback recorded: Good response" : "Feedback recorded: Poor response",
      });
      return { ...prev, [id]: type };
    });
  };

  const handleDeleteMessage = (id: string) => {
    if (loading) return;
    setMessages(messages.filter((m) => m.id !== id));
  };

  const addFiles = async (files: FileList | File[]) => {
    for (const file of Array.from(files)) {
      const ext = file.name.split(".").pop()?.toLowerCase() ?? "";
      const looksTexty =
        TEXT_FILE_EXTENSIONS.has(ext) || file.type.startsWith("text/") || file.type === "application/json";
      if (!looksTexty) {
        addToast({
          type: "error",
          message: `${file.name}: only text-based files are supported.`,
        });
        continue;
      }
      if (file.size > MAX_ATTACHMENT_BYTES) {
        addToast({
          type: "error",
          message: `${file.name}: ${(file.size / 1024).toFixed(0)}KB exceeds limit.`,
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
        addToast({ type: "error", message: `Failed to read ${file.name}.` });
      }
    }
  };

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
    recognition.lang = "en-US";
    recordingBaseRef.current = input;
    finalTranscriptRef.current = "";

    recognition.onresult = (event: any) => {
      let interim = "";
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
        .join(" ");
      setInput(combined);
    };
    recognition.onerror = () => setIsRecording(false);
    recognition.onend = () => setIsRecording(false);

    recognitionRef.current = recognition;
    recognition.start();
    setIsRecording(true);
  }, [isRecording, input]);

  useEffect(() => {
    return () => {
      recognitionRef.current?.stop();
    };
  }, []);

  // Filtered threads for sidebar
  const filteredThreads = useMemo(() => {
    return threads.filter((t) => t.title.toLowerCase().includes(threadSearch.toLowerCase()));
  }, [threads, threadSearch]);

  const pinnedThreads = useMemo(() => filteredThreads.filter((t) => t.pinned), [filteredThreads]);
  const unpinnedThreads = useMemo(() => filteredThreads.filter((t) => !t.pinned), [filteredThreads]);

  void genTick;
  const genElapsedSec = genStartRef.current ? (Date.now() - genStartRef.current) / 1000 : 0;
  const genTokPerSec = genElapsedSec > 0 ? genTokenCountRef.current / genElapsedSec : 0;
  const streamingMsg = streamingId ? messages.find((m) => m.id === streamingId) : undefined;
  const awaitingFirstToken = loading && !!streamingMsg && streamingMsg.content === "";

  const activeNodesCount = metrics?.active_nodes ?? 1;
  const hardwareAttribution = useMemo(() => {
    if (placementPlan?.distributed_active && placementPlan?.tensor_splits?.length > 1) {
      const parts = placementPlan.tensor_splits.map((s: any) => s.label);
      return `Split across ${parts.join(', ')}`;
    }
    return activeNodesCount > 1 ? `Split across cluster (${activeNodesCount} nodes)` : "Local";
  }, [placementPlan, activeNodesCount]);

  return (
    <div className="flex h-full bg-slate-950 text-slate-100 overflow-hidden relative">
      {/* Thread List Sidebar */}
      {showSidebar && (
        <aside className="w-64 border-r border-slate-900 bg-slate-950 flex flex-col flex-shrink-0 z-20">
          <div className="p-3 border-b border-slate-900 flex items-center justify-between">
            <button
              onClick={() => createThread()}
              className="flex items-center gap-2 px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white rounded-xl text-xs font-semibold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              title="New Chat (Ctrl+Shift+O)"
            >
              <Plus size={14} />
              <span>New Chat</span>
              <kbd className="text-[9px] bg-blue-700/50 px-1 py-0.5 rounded font-mono">⌘⇧O</kbd>
            </button>
            <button
              onClick={() => setShowSidebar(false)}
              className="p-1.5 text-slate-400 hover:text-white rounded-lg hover:bg-slate-900 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              title="Close sidebar"
            >
              <PanelLeftClose size={16} />
            </button>
          </div>

          <div className="p-2 border-b border-slate-900">
            <div className="relative">
              <Search size={14} className="absolute left-2.5 top-2.5 text-slate-500" />
              <input
                type="text"
                placeholder="Search threads…"
                value={threadSearch}
                onChange={(e) => setThreadSearch(e.target.value)}
                className="w-full bg-slate-900 border border-slate-800 rounded-lg pl-8 pr-3 py-1 text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
              />
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-2 space-y-3">
            {pinnedThreads.length > 0 && (
              <div>
                <span className="px-2 text-[10px] font-bold text-slate-500 uppercase tracking-wider">Pinned</span>
                <div className="mt-1 space-y-1">
                  {pinnedThreads.map((t) => (
                    <ThreadSidebarItem
                      key={t.id}
                      thread={t}
                      isActive={t.id === activeThreadId}
                      renamingId={renamingThreadId}
                      renameInput={renameTitleInput}
                      setRenameInput={setRenameTitleInput}
                      onSelect={() => selectThread(t.id)}
                      onStartRename={() => {
                        setRenamingThreadId(t.id);
                        setRenameTitleInput(t.title);
                      }}
                      onSaveRename={() => {
                        if (renameTitleInput.trim()) renameThread(t.id, renameTitleInput.trim());
                        setRenamingThreadId(null);
                      }}
                      onCancelRename={() => setRenamingThreadId(null)}
                      onDelete={() => {
                        if (window.confirm(`Delete thread "${t.title}"?`)) deleteThread(t.id);
                      }}
                      onTogglePin={() => togglePinThread(t.id)}
                    />
                  ))}
                </div>
              </div>
            )}

            <div>
              {pinnedThreads.length > 0 && (
                <span className="px-2 text-[10px] font-bold text-slate-500 uppercase tracking-wider">Recent</span>
              )}
              <div className="mt-1 space-y-1">
                {unpinnedThreads.length === 0 && pinnedThreads.length === 0 ? (
                  <div className="p-4 text-center text-xs text-slate-500">No chats found</div>
                ) : (
                  unpinnedThreads.map((t) => (
                    <ThreadSidebarItem
                      key={t.id}
                      thread={t}
                      isActive={t.id === activeThreadId}
                      renamingId={renamingThreadId}
                      renameInput={renameTitleInput}
                      setRenameInput={setRenameTitleInput}
                      onSelect={() => selectThread(t.id)}
                      onStartRename={() => {
                        setRenamingThreadId(t.id);
                        setRenameTitleInput(t.title);
                      }}
                      onSaveRename={() => {
                        if (renameTitleInput.trim()) renameThread(t.id, renameTitleInput.trim());
                        setRenamingThreadId(null);
                      }}
                      onCancelRename={() => setRenamingThreadId(null)}
                      onDelete={() => {
                        if (window.confirm(`Delete thread "${t.title}"?`)) deleteThread(t.id);
                      }}
                      onTogglePin={() => togglePinThread(t.id)}
                    />
                  ))
                )}
              </div>
            </div>
          </div>
        </aside>
      )}

      {/* Main Chat Area */}
      <div className="flex-1 flex flex-col min-w-0 relative">
        {/* Header / Controls */}
        <div className="flex items-center justify-between px-6 py-3 border-b border-slate-900 bg-slate-950/50 backdrop-blur-md sticky top-0 z-10">
          <div className="flex items-center gap-2">
            {!showSidebar && (
              <button
                onClick={() => setShowSidebar(true)}
                className="p-1.5 text-slate-400 hover:text-white rounded-lg hover:bg-slate-900 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                title="Open sidebar"
              >
                <PanelLeft size={16} />
              </button>
            )}

            {/* eslint-disable-next-line jsx-a11y/no-static-element-interactions */}
            <div
              className="relative"
              ref={modelSelectorRef}
              onKeyDown={(e) => { if (e.key === "Escape") setShowModelSelector(false); }}
            >
              <button
                onClick={() => setShowModelSelector(!showModelSelector)}
                aria-haspopup="listbox"
                aria-expanded={showModelSelector}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg hover:bg-slate-900 transition text-sm font-semibold group focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              >
                <span className="text-slate-200 group-hover:text-white max-w-[200px] truncate">
                  {currentModel === "none" ? "Select Model" : currentModel.split("/").pop()}
                </span>
                <ChevronDown size={14} className={`text-slate-500 transition-transform ${showModelSelector ? "rotate-180" : ""}`} aria-hidden="true" />
              </button>

              {showModelSelector && (
                <div role="listbox" aria-label="Available models" className="absolute left-0 mt-2 w-72 bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl z-50 overflow-hidden animate-in fade-in zoom-in-95 duration-100">
                  <div className="p-2 max-h-[400px] overflow-y-auto">
                    {usableModels.length === 0 ? (
                      <div className="p-4 text-center text-slate-500 text-sm">No models available</div>
                    ) : (
                      usableModels.map((m) => (
                        <button
                          key={m.name}
                          role="option"
                          aria-selected={currentModel === m.name}
                          onClick={() => selectModel(m.name)}
                          className={`flex items-center justify-between w-full px-3 py-2.5 rounded-xl text-left transition text-sm ${
                            currentModel === m.name ? "bg-blue-600/10 text-blue-400" : "text-slate-300 hover:bg-slate-800"
                          }`}
                        >
                          <div className="flex flex-col min-w-0">
                            <span className="font-bold truncate">{m.name.split("/").pop()}</span>
                            <span className="text-[10px] text-slate-500 truncate">{m.name}</span>
                          </div>
                          {currentModel === m.name && <Check size={14} aria-hidden="true" />}
                        </button>
                      ))
                    )}
                  </div>
                </div>
              )}
            </div>

            {/* Header Status Badges */}
            <div className="flex items-center gap-2 border-l border-slate-800 pl-3">
              <span className="text-[11px] bg-slate-900 border border-slate-800 px-2 py-0.5 rounded-md text-slate-400">
                Backend: <strong className="text-slate-200">{selectedEngine?.label || "Native"}</strong>
              </span>
              {selectedEngine?.capabilities?.structured_outputs && (
                <span className="text-[11px] bg-slate-900 border border-slate-800 px-2 py-0.5 rounded-md text-slate-400">
                  Structured outputs
                </span>
              )}
              {!toolCallsSupported && (
                <span className="text-[11px] bg-amber-500/10 text-amber-400 border border-amber-500/20 px-2 py-0.5 rounded-md font-semibold">
                  No tool calls
                </span>
              )}
              <span className="text-[11px] bg-slate-900 border border-slate-800 px-2 py-0.5 rounded-md text-slate-400">
                Context: <strong className="text-slate-200">Estimated: {historyTokens} / {conversationTokenLimit} tokens</strong>
              </span>
              <span className="text-[11px] bg-slate-900 border border-slate-800 px-2 py-0.5 rounded-md text-slate-400">
                Hardware: <strong className="text-slate-200">{hardwareAttribution}</strong>
              </span>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={() => setShowKnobs(!showKnobs)}
              className={`p-2 rounded-lg transition text-slate-400 hover:text-white focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
                showKnobs ? "bg-slate-800 text-white" : "hover:bg-slate-900"
              }`}
              title="Per-thread settings & system prompt presets"
            >
              <SlidersHorizontal size={16} />
            </button>
          </div>
        </div>

        {/* Knobs Drawer / Bar */}
        {showKnobs && (
          <div className="bg-slate-900/90 border-b border-slate-800 p-4 space-y-4 text-xs animate-in slide-in-from-top-2 duration-150">
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div>
                <label htmlFor="system-preset-select" className="block text-slate-400 font-semibold mb-1">System Prompt Preset</label>
                <select
                  id="system-preset-select"
                  value={selectedPresetId}
                  onChange={(e) => {
                    const pid = e.target.value;
                    setSelectedPresetId(pid);
                    const preset = presets.find((p) => p.id === pid);
                    if (preset && activeThreadId) {
                      updateActiveThread((t) => ({ ...t, systemPrompt: preset.prompt }));
                    }
                  }}
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-slate-200 focus:outline-none focus:ring-1 focus:ring-blue-500"
                >
                  {presets.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <label htmlFor="thread-temp-input" className="block text-slate-400 font-semibold mb-1">Temperature ({threadTemperature})</label>
                <input
                  id="thread-temp-input"
                  type="range"
                  min="0"
                  max="2"
                  step="0.1"
                  value={threadTemperature}
                  onChange={(e) => {
                    const temp = parseFloat(e.target.value);
                    if (activeThreadId) updateActiveThread((t) => ({ ...t, temperature: temp }));
                  }}
                  className="w-full accent-blue-500"
                />
              </div>

              <div>
                <label htmlFor="thread-maxtokens-input" className="block text-slate-400 font-semibold mb-1">Max Tokens</label>
                <input
                  id="thread-maxtokens-input"
                  type="number"
                  min="64"
                  max="8192"
                  step="64"
                  value={threadMaxTokens}
                  onChange={(e) => {
                    const mt = parseInt(e.target.value, 10) || 2048;
                    if (activeThreadId) updateActiveThread((t) => ({ ...t, maxTokens: mt }));
                  }}
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-slate-200 focus:outline-none focus:ring-1 focus:ring-blue-500"
                />
              </div>
            </div>

            <details className="group">
              <summary className="cursor-pointer text-slate-400 hover:text-slate-200 font-semibold flex items-center gap-1">
                <span>Advanced Parameters (Top-P, Repeat Penalty)</span>
                <ChevronDown size={14} className="group-open:rotate-180 transition-transform" />
              </summary>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mt-3 pt-3 border-t border-slate-800/50">
                <div>
                  <label htmlFor="thread-topp-input" className="block text-slate-400 font-semibold mb-1">Top-P ({threadTopP})</label>
                  <input
                    id="thread-topp-input"
                    type="range"
                    min="0"
                    max="1"
                    step="0.05"
                    value={threadTopP}
                    onChange={(e) => {
                      const tp = parseFloat(e.target.value);
                      if (activeThreadId) updateActiveThread((t) => ({ ...t, top_p: tp }));
                    }}
                    className="w-full accent-blue-500"
                  />
                </div>
                <div>
                  <label htmlFor="thread-penalty-input" className="block text-slate-400 font-semibold mb-1">Repeat Penalty ({threadPenalty})</label>
                  <input
                    id="thread-penalty-input"
                    type="range"
                    min="1.0"
                    max="2.0"
                    step="0.05"
                    value={threadPenalty}
                    onChange={(e) => {
                      const pen = parseFloat(e.target.value);
                      if (activeThreadId) updateActiveThread((t) => ({ ...t, penalty: pen }));
                    }}
                    className="w-full accent-blue-500"
                  />
                </div>
              </div>
            </details>
          </div>
        )}

        {/* Message Stream Announcement for Accessibility */}
        {streamAnnouncement && <div className="sr-only" aria-live="polite">{streamAnnouncement}</div>}

        {/* Chat Transcript Container */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {messages.length === 0 ? (
            <div className="max-w-2xl mx-auto py-12 text-center space-y-6">
              <div className="w-16 h-16 bg-blue-600/10 text-blue-400 rounded-3xl flex items-center justify-center mx-auto border border-blue-500/20 shadow-xl">
                <Sparkles size={32} />
              </div>
              <div className="space-y-2">
                <h2 className="text-2xl font-bold text-white">How can I help you today?</h2>
                <p className="text-slate-400 text-sm max-w-md mx-auto">
                  Ghostlink Studio co-pilot is ready for multi-turn chat, code generation, and distributed fabric orchestration.
                </p>
              </div>

              {/* CTAs */}
              <div className="flex items-center justify-center gap-3 pt-2">
                <button
                  onClick={() => textareaRef.current?.focus()}
                  className="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded-xl text-sm font-semibold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                >
                  Start a chat
                </button>
                {currentModel === "none" && (
                  <button
                    onClick={() => setActiveTab(1)}
                    className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded-xl text-sm font-semibold transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  >
                    Load a model
                  </button>
                )}
              </div>

              {/* Suggestion Chips */}
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-4 max-w-lg mx-auto text-left">
                {SUGGESTIONS.map((s) => (
                  <button
                    key={s.text}
                    onClick={() => {
                      setInput(s.text);
                      textareaRef.current?.focus();
                    }}
                    className="p-3 bg-slate-900/60 hover:bg-slate-900 border border-slate-800 rounded-xl transition text-xs text-slate-300 hover:text-white focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  >
                    {s.text}
                  </button>
                ))}
              </div>
            </div>
          ) : (
            <div className="max-w-4xl mx-auto space-y-6">
              {messages.map((m) => {
                if (m.isDivider) {
                  return (
                    <div key={m.id} className="flex items-center gap-3 my-4">
                      <div className="flex-1 border-t border-slate-800" />
                      <span className="text-[11px] font-semibold text-slate-500 uppercase tracking-wider px-2 bg-slate-900/80 border border-slate-800 rounded-full py-0.5">
                        {m.content}
                      </span>
                      <div className="flex-1 border-t border-slate-800" />
                    </div>
                  );
                }

                const isUser = m.role === "user";
                const isEditing = editingMsgId === m.id;

                return (
                  <div
                    key={m.id}
                    className={`flex gap-4 ${isUser ? "flex-row-reverse" : "flex-row"} group`}
                  >
                    <div
                      className={`w-8 h-8 rounded-xl flex items-center justify-center flex-shrink-0 text-white font-bold text-xs shadow-md ${
                        isUser ? "bg-blue-600" : "bg-purple-600"
                      }`}
                    >
                      {isUser ? <User size={16} /> : <Bot size={16} />}
                    </div>

                    <div className={`flex-1 min-w-0 max-w-[85%] ${isUser ? "text-right" : "text-left"}`}>
                      <div className="flex items-center gap-2 mb-1 text-[11px] text-slate-500">
                        <span className="font-semibold text-slate-400">{isUser ? "You" : "Assistant"}</span>
                        <span>{m.timestamp}</span>
                        {m.model && <span className="text-[10px] bg-slate-900 border border-slate-800 px-1.5 py-0.2 rounded">{m.model.split("/").pop()}</span>}
                      </div>

                      {isEditing ? (
                        <div className="space-y-2 bg-slate-900 p-3 rounded-2xl border border-slate-800">
                          <textarea
                            value={editingMsgText}
                            onChange={(e) => setEditingMsgText(e.target.value)}
                            className="w-full bg-slate-950 border border-slate-800 rounded-xl p-2.5 text-xs text-slate-200 focus:outline-none focus:ring-1 focus:ring-blue-500"
                            rows={3}
                          />
                          <div className="flex justify-end gap-2">
                            <button
                              onClick={() => setEditingMsgId(null)}
                              className="px-3 py-1 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg text-xs"
                            >
                              Cancel
                            </button>
                            <button
                              onClick={() => handleSaveUserEdit(m.id)}
                              className="px-3 py-1 bg-blue-600 hover:bg-blue-500 text-white rounded-lg text-xs font-semibold"
                            >
                              Save & Regenerate
                            </button>
                          </div>
                        </div>
                      ) : (
                        <div
                          className={`p-4 rounded-2xl border text-sm leading-relaxed ${
                            isUser
                              ? "bg-blue-600/10 border-blue-500/20 text-slate-100"
                              : "bg-slate-900/60 border-slate-800/80 text-slate-200 shadow-md"
                          }`}
                        >
                          <div className={MARKDOWN_PROSE_CLASSES}>
                            <ReactMarkdown
                              remarkPlugins={[remarkGfm]}
                              rehypePlugins={[rehypeHighlight]}
                              components={{
                                code: CodeBlock,
                                table: MarkdownTable,
                              }}
                            >
                              {m.content}
                            </ReactMarkdown>
                          </div>

                          {/* Controls Footer */}
                          <div className="flex items-center gap-2 mt-3 pt-2 border-t border-slate-800/40 text-xs text-slate-500">
                            <button
                              onClick={() => handleCopy(m.content, m.id)}
                              className="p-1 hover:text-white rounded transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                              aria-label={copiedId === m.id ? "Copied response to clipboard" : "Copy response"}
                              title={copiedId === m.id ? "Copied!" : "Copy response"}
                            >
                              {copiedId === m.id ? <Check size={14} className="text-green-400" /> : <Copy size={14} />}
                            </button>

                            {isUser ? (
                              <button
                                onClick={() => {
                                  setEditingMsgId(m.id);
                                  setEditingMsgText(m.content);
                                }}
                                className="p-1 hover:text-white rounded transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                                title="Edit message"
                              >
                                <Pencil size={14} />
                              </button>
                            ) : (
                              <>
                                <button
                                  onClick={() => handleRegenerateAssistant(m.id)}
                                  className="p-1 hover:text-white rounded transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                                  title="Regenerate assistant response"
                                >
                                  <RotateCcw size={14} />
                                </button>
                                <button
                                  onClick={() => handleRating(m.id, "up")}
                                  className={`p-1 rounded transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
                                    messageRatings[m.id] === "up" ? "text-green-400 bg-green-500/10" : "hover:text-green-400"
                                  }`}
                                  aria-label={messageRatings[m.id] === "up" ? "Rated as good response" : "Rate as good response"}
                                  aria-pressed={messageRatings[m.id] === "up"}
                                  title="Good response"
                                >
                                  <ThumbsUp size={14} />
                                </button>
                                <button
                                  onClick={() => handleRating(m.id, "down")}
                                  className={`p-1 rounded transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
                                    messageRatings[m.id] === "down" ? "text-red-400 bg-red-500/10" : "hover:text-red-400"
                                  }`}
                                  aria-label={messageRatings[m.id] === "down" ? "Rated as poor response" : "Rate as poor response"}
                                  aria-pressed={messageRatings[m.id] === "down"}
                                  title="Poor response"
                                >
                                  <ThumbsDown size={14} />
                                </button>
                              </>
                            )}

                            <button
                              onClick={() => handleDeleteMessage(m.id)}
                              className="p-1 hover:text-red-400 rounded transition ml-auto focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                              title="Delete message"
                            >
                              <Trash2 size={14} />
                            </button>
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}

              {loading && awaitingFirstToken && (
                <div className="flex items-center gap-3 text-slate-400 text-xs py-2">
                  <Loader size={16} className="animate-spin text-blue-500" />
                  <span>Generating response... ({genTokPerSec.toFixed(1)} tok/s)</span>
                </div>
              )}

              <div ref={messagesEndRef} />
            </div>
          )}
        </div>

        {/* Prompt Library Overlay */}
        {showPromptLibrary && (
          <div className="absolute bottom-20 left-6 right-6 max-w-xl mx-auto bg-slate-900 border border-slate-800 rounded-2xl p-4 shadow-2xl z-30 animate-in fade-in zoom-in-95 duration-100">
            <div className="flex items-center justify-between pb-3 border-b border-slate-800">
              <span className="font-bold text-xs text-slate-200 flex items-center gap-2">
                <BookOpen size={14} className="text-blue-400" />
                Prompt Templates
              </span>
              <button
                onClick={() => setShowPromptLibrary(false)}
                className="text-slate-400 hover:text-white"
              >
                <X size={14} />
              </button>
            </div>
            <div className="mt-3 max-h-48 overflow-y-auto space-y-1.5">
              {userPrompts.map((p) => (
                <button
                  key={p.id}
                  onClick={() => {
                    setInput(p.content);
                    setShowPromptLibrary(false);
                    textareaRef.current?.focus();
                  }}
                  className="w-full text-left p-2.5 rounded-xl hover:bg-slate-800 transition text-xs border border-slate-800/50 flex flex-col gap-0.5"
                >
                  <span className="font-bold text-slate-200">{p.title}</span>
                  <span className="text-slate-400 truncate">{p.content}</span>
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Composer Input Area */}
        <div className="p-4 border-t border-slate-900 bg-slate-950/80 backdrop-blur-md">
          <div className="max-w-4xl mx-auto space-y-3">
            {!toolCallsSupported && (
              <div className="px-3 py-1.5 rounded-xl bg-amber-500/10 border border-amber-500/20 text-xs text-amber-300">
                {selectedEngine?.label || "Current engine"} does not support tool calling. Tool selections are disabled for this chat engine.
              </div>
            )}
            {/* Attachment Chips */}
            {attachments.length > 0 && (
              <div className="flex flex-wrap gap-2">
                {attachments.map((a) => (
                  <span
                    key={a.id}
                    className="inline-flex items-center gap-1.5 px-2.5 py-1 bg-slate-900 border border-slate-800 rounded-lg text-xs text-slate-300"
                  >
                    <FileText size={12} className="text-blue-400" />
                    <span className="truncate max-w-[150px]">{a.name}</span>
                    <button
                      onClick={() => setAttachments((prev) => prev.filter((x) => x.id !== a.id))}
                      className="text-slate-500 hover:text-white"
                    >
                      <X size={12} />
                    </button>
                  </span>
                ))}
              </div>
            )}

            <div
              onDragOver={(e) => {
                e.preventDefault();
                setIsDraggingFile(true);
              }}
              onDragLeave={() => setIsDraggingFile(false)}
              onDrop={(e) => {
                e.preventDefault();
                setIsDraggingFile(false);
                if (e.dataTransfer.files?.length) addFiles(e.dataTransfer.files);
              }}
              className={`relative rounded-2xl border transition ${
                isDraggingFile ? "border-blue-500 bg-blue-500/10" : "border-slate-800 bg-slate-900/60"
              }`}
            >
              <textarea
                ref={textareaRef}
                value={input}
                onChange={(e) => {
                  setInput(e.target.value);
                  if (e.target.value === "/") {
                    setShowPromptLibrary(true);
                  }
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    handleSend();
                  } else if (e.key === "Escape" && loading) {
                    e.preventDefault();
                    handleStop();
                  }
                }}
                placeholder="Send a Message"
                rows={3}
                className="w-full bg-transparent p-4 pr-24 text-sm text-slate-100 placeholder-slate-500 focus:outline-none resize-none"
              />

              <input
                ref={fileInputRef}
                type="file"
                multiple
                onChange={(e) => {
                  if (e.target.files) addFiles(e.target.files);
                  e.target.value = "";
                }}
                className="hidden"
              />

              <div className="absolute right-3 bottom-3 flex items-center gap-1.5">
                <button
                  type="button"
                  disabled={!toolCallsSupported}
                  onClick={() => setShowPromptLibrary(!showPromptLibrary)}
                  className="p-2 text-slate-400 hover:text-white rounded-xl hover:bg-slate-800 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none disabled:opacity-40 disabled:cursor-not-allowed"
                  title={toolCallsSupported ? "Select tools" : "Tool calling is unavailable for this engine"}
                >
                  <Wrench size={16} />
                </button>
                <button
                  type="button"
                  onClick={() => setShowPromptLibrary(!showPromptLibrary)}
                  className="p-2 text-slate-400 hover:text-white rounded-xl hover:bg-slate-800 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  title="Prompt Templates (/)"
                >
                  <BookOpen size={16} />
                </button>

                <button
                  type="button"
                  onClick={() => fileInputRef.current?.click()}
                  className="p-2 text-slate-400 hover:text-white rounded-xl hover:bg-slate-800 transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                  title="Attach text file"
                >
                  <Paperclip size={16} />
                </button>

                {getSpeechRecognitionCtor() && (
                  <button
                    type="button"
                    onClick={toggleRecording}
                    className={`p-2 rounded-xl transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
                      isRecording ? "bg-red-500/20 text-red-400 animate-pulse" : "text-slate-400 hover:text-white hover:bg-slate-800"
                    }`}
                    aria-label={isRecording ? "Stop voice input" : "Start voice input"}
                    title={isRecording ? "Stop voice input" : "Start voice input"}
                  >
                    <Mic size={16} />
                  </button>
                )}

                {loading ? (
                  <button
                    type="button"
                    onClick={handleStop}
                    className="p-2 bg-red-600 hover:bg-red-500 text-white rounded-xl transition shadow-lg focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                    title="Stop generation"
                  >
                    <Square size={16} />
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={handleSend}
                    disabled={!input.trim() && attachments.length === 0}
                    className="p-2 bg-blue-600 hover:bg-blue-500 disabled:bg-slate-800 disabled:text-slate-600 text-white rounded-xl transition shadow-lg focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
                    title="Send message"
                  >
                    <Send size={16} />
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

interface ThreadItemProps {
  thread: Thread;
  isActive: boolean;
  renamingId: string | null;
  renameInput: string;
  setRenameInput: (val: string) => void;
  onSelect: () => void;
  onStartRename: () => void;
  onSaveRename: () => void;
  onCancelRename: () => void;
  onDelete: () => void;
  onTogglePin: () => void;
}

const ThreadSidebarItem: React.FC<ThreadItemProps> = ({
  thread,
  isActive,
  renamingId,
  renameInput,
  setRenameInput,
  onSelect,
  onStartRename,
  onSaveRename,
  onCancelRename,
  onDelete,
  onTogglePin,
}) => {
  const isRenaming = renamingId === thread.id;

  if (isRenaming) {
    return (
      <div className="p-1 bg-slate-900 rounded-lg flex items-center gap-1">
        <input
          type="text"
          value={renameInput}
          onChange={(e) => setRenameInput(e.target.value)}
          aria-label={`Rename thread ${thread.title}`}
          className="w-full bg-slate-950 border border-slate-800 rounded px-2 py-1 text-xs text-slate-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500"
          // eslint-disable-next-line jsx-a11y/no-autofocus
          autoFocus
          onKeyDown={(e) => {
            if (e.key === "Enter") onSaveRename();
            if (e.key === "Escape") onCancelRename();
          }}
        />
        <button
          type="button"
          onClick={onSaveRename}
          className="p-1 text-green-400 hover:text-green-300 rounded focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          title="Save title"
          aria-label="Save thread title"
        >
          <Check size={14} aria-hidden="true" />
        </button>
      </div>
    );
  }

  return (
    <div
      className={`group flex items-center justify-between px-2.5 py-1.5 rounded-xl text-xs transition ${
        isActive ? "bg-blue-600/10" : "hover:bg-slate-900"
      }`}
    >
      <button
        type="button"
        onClick={onSelect}
        aria-label={`Select thread ${thread.title}`}
        className={`flex items-center gap-2 min-w-0 flex-1 text-left rounded-lg transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none ${
          isActive ? "text-blue-400 font-semibold" : "text-slate-400 hover:text-slate-200"
        }`}
      >
        <MessageSquare size={13} className="flex-shrink-0" aria-hidden="true" />
        <span className="truncate">{thread.title}</span>
      </button>
      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition">
        <button
          type="button"
          onClick={onTogglePin}
          className="p-1 text-slate-400 hover:text-white rounded focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          title={thread.pinned ? "Unpin thread" : "Pin thread"}
          aria-label={thread.pinned ? `Unpin thread ${thread.title}` : `Pin thread ${thread.title}`}
        >
          {thread.pinned ? <PinOff size={12} aria-hidden="true" /> : <Pin size={12} aria-hidden="true" />}
        </button>
        <button
          type="button"
          onClick={onStartRename}
          className="p-1 text-slate-400 hover:text-white rounded focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          title="Rename thread"
          aria-label={`Rename thread ${thread.title}`}
        >
          <Pencil size={12} aria-hidden="true" />
        </button>
        <button
          type="button"
          onClick={onDelete}
          className="p-1 text-slate-400 hover:text-red-400 rounded focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
          title="Delete thread"
          aria-label={`Delete thread ${thread.title}`}
        >
          <Trash2 size={12} aria-hidden="true" />
        </button>
      </div>
    </div>
  );
};
