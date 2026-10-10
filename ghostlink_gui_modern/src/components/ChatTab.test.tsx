import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { ChatTab } from './ChatTab';
import { useAppStore } from '../store';
import { GhostlinkAPI } from '../api';

function createMockApi(engine: 'ollama' | 'native' | 'vllm' = 'ollama'): GhostlinkAPI {
  const api = new GhostlinkAPI('http://localhost:8003');
  vi.spyOn(api, 'getModels').mockResolvedValue({ models: [], current_model: 'none' });
  vi.spyOn(api, 'sendMessage').mockResolvedValue({ success: true, data: { response: 'Hello from test' } });
  vi.spyOn(api, 'loadModel').mockResolvedValue({ success: true, data: {} });
  vi.spyOn(api, 'listSessions').mockResolvedValue({ sessions: [] });
  vi.spyOn(api, 'getOllamaHealth').mockResolvedValue({ reachable: false, model_count: 0 });
  vi.spyOn(api, 'getVllmHealth').mockResolvedValue({ reachable: false, model_count: 0 });
  vi.spyOn(api, 'getInferenceEngines').mockResolvedValue({
    current: engine,
    engines: [
      {
        name: 'ollama',
        label: 'Ollama',
        status: engine === 'ollama' ? 'active' : 'ready',
        default_base_url: 'http://127.0.0.1:11434',
        capabilities: {
          streaming: true,
          model_listing: true,
          model_load: true,
          model_unload: true,
          structured_outputs: false,
          tool_calls: false,
        },
      },
      {
        name: 'native',
        label: 'Native',
        status: engine === 'native' ? 'active' : 'ready',
        default_base_url: null,
        capabilities: {
          streaming: true,
          model_listing: false,
          model_load: true,
          model_unload: true,
          structured_outputs: false,
          tool_calls: false,
        },
      },
      {
        name: 'vllm',
        label: 'vLLM',
        status: engine === 'vllm' ? 'active' : 'ready',
        default_base_url: 'http://127.0.0.1:8000',
        capabilities: {
          streaming: true,
          model_listing: true,
          model_load: true,
          model_unload: false,
          structured_outputs: true,
          tool_calls: true,
        },
      },
    ],
  });
  return api;
}

describe('ChatTab', () => {
  beforeEach(() => {
    useAppStore.setState({
      currentModel: 'none',
      models: [],
      apiBase: 'http://localhost:8003',
      backendOnline: false,
      uptime: 0,
      metrics: null,
      sessions: [],
      workers: [],
      selectedModel: null,
      activeTab: 0,
      setApiBase: vi.fn(),
      setBackendOnline: vi.fn(),
      setCurrentModel: vi.fn(),
      setUptime: vi.fn(),
      setModels: vi.fn(),
      setMetrics: vi.fn(),
      setSessions: vi.fn(),
      setWorkers: vi.fn(),
      setSelectedModel: vi.fn(),
      setActiveTab: vi.fn(),
    });
  });

  // Context indicators. The two states are deliberately distinct: `truncated`
  // means older turns were dropped to fit THIS reply's token limit, while
  // `summarized_history` means the model answered from a running summary of turns
  // trimmed in EARLIER requests. Merging them would hide the case where the gap
  // is still growing.
  function seedAssistantMessage(extra: Record<string, unknown>) {
    useAppStore.setState({
      chatMessages: [
        { role: 'user', content: 'hi', id: 'u1', timestamp: '10:00' },
        { role: 'assistant', content: 'hello', id: 'a1', timestamp: '10:00', ...extra },
      ],
    } as never);
  }

  afterEach(() => {
    useAppStore.setState({ chatMessages: [] } as never);
  });

  it('shows the dropped-turns indicator when the server trimmed this turn', () => {
    seedAssistantMessage({ truncatedBefore: true });
    render(<ChatTab api={createMockApi()} />);
    expect(
      screen.getByLabelText(/earlier turns were dropped from this reply/i)
    ).toBeInTheDocument();
  });

  it('shows the summary indicator when answering from condensed memory', () => {
    seedAssistantMessage({ summarizedHistory: true });
    render(<ChatTab api={createMockApi()} />);
    expect(
      screen.getByLabelText(/produced from a condensed summary of earlier turns/i)
    ).toBeInTheDocument();
  });

  it('shows both indicators when both conditions apply', () => {
    seedAssistantMessage({ truncatedBefore: true, summarizedHistory: true });
    render(<ChatTab api={createMockApi()} />);
    expect(
      screen.getByLabelText(/earlier turns were dropped from this reply/i)
    ).toBeInTheDocument();
    expect(
      screen.getByLabelText(/produced from a condensed summary of earlier turns/i)
    ).toBeInTheDocument();
  });

  it('shows how many memories were recalled, without their content', () => {
    seedAssistantMessage({ recalledMemories: 3 });
    render(<ChatTab api={createMockApi()} />);
    expect(
      screen.getByLabelText(/3 stored memories were used as context/i)
    ).toBeInTheDocument();
    // The count is shown; the recalled text never reaches the client at all, so
    // there is nothing to assert beyond the count being present.
    expect(screen.getByText(/3 memories recalled/i)).toBeInTheDocument();
  });

  it('uses the singular form for a single recalled memory', () => {
    seedAssistantMessage({ recalledMemories: 1 });
    render(<ChatTab api={createMockApi()} />);
    expect(
      screen.getByLabelText(/1 stored memory was used as context/i)
    ).toBeInTheDocument();
  });

  it('shows how many indexed documents were retrieved', () => {
    seedAssistantMessage({ recalledDocuments: 2 });
    render(<ChatTab api={createMockApi()} />);
    expect(
      screen.getByLabelText(/2 indexed documents were retrieved/i)
    ).toBeInTheDocument();
  });

  it('flags a reply whose action claim the server corrected', () => {
    seedAssistantMessage({ actionClaimCorrected: true });
    render(<ChatTab api={createMockApi()} />);
    expect(
      screen.getByLabelText(/claimed an action that never ran/i)
    ).toBeInTheDocument();
  });

  it('shows no recall badge when nothing was recalled', () => {
    seedAssistantMessage({ recalledMemories: 0, recalledDocuments: 0 });
    render(<ChatTab api={createMockApi()} />);
    expect(screen.queryByText(/memories recalled/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/documents found/i)).not.toBeInTheDocument();
    expect(
      screen.queryByLabelText(/claimed an action that never ran/i)
    ).not.toBeInTheDocument();
  });

  it('never shows recall or correction badges on a user message', () => {
    useAppStore.setState({
      chatMessages: [
        {
          role: 'user',
          content: 'remember this',
          id: 'u1',
          timestamp: '10:00',
          recalledMemories: 5,
          actionClaimCorrected: true,
        },
        { role: 'assistant', content: 'ok', id: 'a1', timestamp: '10:00' },
      ],
    } as never);
    render(<ChatTab api={createMockApi()} />);
    expect(screen.queryByText(/memories recalled/i)).not.toBeInTheDocument();
    expect(
      screen.queryByLabelText(/claimed an action that never ran/i)
    ).not.toBeInTheDocument();
  });

  it('shows neither indicator on an ordinary reply', () => {
    seedAssistantMessage({});
    render(<ChatTab api={createMockApi()} />);
    expect(
      screen.queryByLabelText(/earlier turns were dropped from this reply/i)
    ).not.toBeInTheDocument();
    expect(
      screen.queryByLabelText(/produced from a condensed summary of earlier turns/i)
    ).not.toBeInTheDocument();
  });

  it('never shows context indicators on a user message', () => {
    useAppStore.setState({
      chatMessages: [
        { role: 'user', content: 'hi', id: 'u1', timestamp: '10:00', summarizedHistory: true },
        { role: 'assistant', content: 'hello', id: 'a1', timestamp: '10:00' },
      ],
    } as never);
    render(<ChatTab api={createMockApi()} />);
    expect(
      screen.queryByLabelText(/produced from a condensed summary of earlier turns/i)
    ).not.toBeInTheDocument();
  });

  it('renders the chat interface', () => {
    const api = createMockApi();
    render(<ChatTab api={api} />);
    expect(screen.getByText('How can I help you today?')).toBeInTheDocument();
  });

  it('shows model selector when no model loaded', () => {
    const api = createMockApi();
    render(<ChatTab api={api} />);
    expect(screen.getByText('Select Model')).toBeInTheDocument();
  });

  it('shows send button and accessible composer textarea', () => {
    const api = createMockApi();
    render(<ChatTab api={api} />);
    const textarea = screen.getByLabelText('Chat message input');
    expect(textarea).toBeInTheDocument();
    expect(textarea).toHaveAttribute('placeholder', 'Send a Message');
  });

  it('renders accessible suggestion chips with aria-label and title tooltips on empty state', () => {
    const api = createMockApi();
    render(<ChatTab api={api} />);
    const suggestionBtn = screen.getByRole('button', { name: 'Ask about active cluster node health' });
    expect(suggestionBtn).toBeInTheDocument();
    expect(suggestionBtn).toHaveAttribute('title', 'Ask about active cluster node health');
  });

  it('allows typing a message', () => {
    const api = createMockApi();
    render(<ChatTab api={api} />);
    const textarea = screen.getByPlaceholderText(/Send a Message/i);
    fireEvent.change(textarea, { target: { value: 'Hello world' } });
    expect(textarea).toHaveValue('Hello world');
  });

  it('shows model name when a model is loaded', () => {
    useAppStore.setState({ currentModel: 'llama-3-8b', models: [{ name: 'llama-3-8b', size_gb: 8, type: 'LLM', quantization: 'Q4', status: 'Loaded', usable: true }] });
    const api = createMockApi();
    render(<ChatTab api={api} />);
    expect(screen.getByText('llama-3-8b')).toBeInTheDocument();
  });

  it('disables tool calling controls when the engine lacks tool support', async () => {
    const api = createMockApi('native');
    render(<ChatTab api={api} />);

    expect(await screen.findByText(/No tool calls/i)).toBeInTheDocument();
    expect(screen.getByText(/does not support tool calling/i)).toBeInTheDocument();
    expect(screen.getByTitle('Tool calling is unavailable for this engine')).toBeDisabled();
  });

  it('shows structured output support for vllm', async () => {
    const api = createMockApi('vllm');
    render(<ChatTab api={api} />);

    expect(await screen.findByText('Structured outputs')).toBeInTheDocument();
    expect(screen.getByText('vLLM')).toBeInTheDocument();
  });

  it('allows rating assistant responses with thumbs up / thumbs down', () => {
    useAppStore.setState({
      chatMessages: [
        { role: 'assistant', content: 'Test response', id: 'msg-1', timestamp: '12:00 PM' }
      ]
    });
    const api = createMockApi();
    render(<ChatTab api={api} />);

    const thumbsUp = screen.getByLabelText('Rate as good response');
    const thumbsDown = screen.getByLabelText('Rate as poor response');

    expect(thumbsUp).toHaveAttribute('aria-pressed', 'false');
    expect(thumbsDown).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(thumbsUp);
    expect(screen.getByLabelText('Rated as good response')).toHaveAttribute('aria-pressed', 'true');

    fireEvent.click(thumbsDown);
    expect(screen.getByLabelText('Rated as poor response')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByLabelText('Rate as good response')).toHaveAttribute('aria-pressed', 'false');
  });

  it('updates copy button aria-label and title when response is copied in markdown code block or compare mode', () => {
    useAppStore.setState({
      chatMessages: [
        {
          role: 'assistant',
          content: '```js\nconsole.log("hello");\n```',
          id: 'msg-code',
          timestamp: '12:00 PM',
        },
        {
          role: 'assistant',
          content: 'Compare reply A',
          id: 'cmp-1-a',
          timestamp: '12:01 PM',
          compareGroupId: 'cmp-1',
        },
        {
          role: 'assistant',
          content: 'Compare reply B',
          id: 'cmp-1-b',
          timestamp: '12:01 PM',
          compareGroupId: 'cmp-1',
        },
      ],
    });
    const api = createMockApi();
    render(<ChatTab api={api} />);

    const codeCopyBtn = screen.getByLabelText('Copy code');
    expect(codeCopyBtn).toHaveAttribute('title', 'Copy code');

    const compareCopyBtns = screen.getAllByLabelText('Copy response');
    expect(compareCopyBtns.length).toBeGreaterThan(0);
    expect(compareCopyBtns[0]).toHaveAttribute('title', 'Copy response');
  });

  describe('voice input', () => {
    afterEach(() => {
      vi.unstubAllGlobals();
    });

    it('does not render the mic button when SpeechRecognition is unavailable', () => {
      const api = createMockApi();
      render(<ChatTab api={api} />);
      expect(screen.queryByLabelText('Start voice input')).not.toBeInTheDocument();
    });

    it('renders the mic button and starts/stops recognition on click', () => {
      const instances: any[] = [];
      class MockSpeechRecognition {
        continuous = false;
        interimResults = false;
        lang = '';
        onresult: ((e: any) => void) | null = null;
        onerror: (() => void) | null = null;
        onend: (() => void) | null = null;
        start = vi.fn();
        stop = vi.fn(() => {
          this.onend?.();
        });
        constructor() {
          instances.push(this);
        }
      }
      vi.stubGlobal('SpeechRecognition', MockSpeechRecognition);

      const api = createMockApi();
      render(<ChatTab api={api} />);

      const micButton = screen.getByLabelText('Start voice input');
      fireEvent.click(micButton);

      expect(instances).toHaveLength(1);
      expect(instances[0].start).toHaveBeenCalledOnce();
      expect(screen.getByLabelText('Stop voice input')).toBeInTheDocument();

      // A final transcript result should land in the textarea. `results[i]`
      // mimics a SpeechRecognitionResult: array-indexable to an alternative
      // with `.transcript`, plus an `.isFinal` flag.
      const finalResult = Object.assign([{ transcript: 'hello from voice' }], { isFinal: true });
      act(() => {
        instances[0].onresult({ resultIndex: 0, results: [finalResult] });
      });
      const textarea = screen.getByPlaceholderText(/Send a Message/i);
      expect(textarea).toHaveValue('hello from voice');

      fireEvent.click(screen.getByLabelText('Stop voice input'));
      expect(instances[0].stop).toHaveBeenCalledOnce();
      expect(screen.getByLabelText('Start voice input')).toBeInTheDocument();
    });
  });

  describe("Phase 3 Studio Chat features", () => {
    it("renders empty state CTAs and navigates to Models tab when CTA clicked", () => {
      useAppStore.setState({ currentModel: "none", chatMessages: [] });
      const api = createMockApi();
      render(<ChatTab api={api} />);

      expect(screen.getByText("Start a chat")).toBeInTheDocument();
      const loadModelBtn = screen.getByText("Load a model");
      expect(loadModelBtn).toBeInTheDocument();

      fireEvent.click(loadModelBtn);
      expect(useAppStore.getState().setActiveTab).toHaveBeenCalledWith(1);
    });

    it("renders accessible status live region during response generation", async () => {
      useAppStore.setState({
        currentModel: "llama-3-8b",
        chatLoading: true,
        chatStreamingId: "ast-1",
        chatMessages: [
          { role: "user", content: "Hello", id: "u1", timestamp: "12:00 PM" },
          { role: "assistant", content: "", id: "ast-1", timestamp: "12:00 PM" },
        ],
      });
      const api = createMockApi();
      render(<ChatTab api={api} />);

      const statusEl = screen.getByRole("status");
      expect(statusEl).toBeInTheDocument();
      expect(statusEl).toHaveAttribute("aria-live", "polite");
      expect(statusEl).toHaveAttribute("aria-busy", "true");
      expect(screen.getByText(/Generating response.../i)).toBeInTheDocument();

      useAppStore.setState({ chatLoading: false, chatStreamingId: null, chatMessages: [] });
    });

    it("triggers send message and streams tokens", async () => {
      useAppStore.setState({ currentModel: "llama-3-8b" });
      const api = createMockApi();
      render(<ChatTab api={api} />);

      const textarea = screen.getByPlaceholderText(/Send a Message/i);
      fireEvent.change(textarea, { target: { value: "Hello co-pilot" } });
      const sendBtn = screen.getByTitle("Send message");

      await act(async () => {
        fireEvent.click(sendBtn);
      });

      expect(api.sendMessage).toHaveBeenCalled();
    });

    it("edits user message and truncates following turns", async () => {
      useAppStore.setState({
        chatMessages: [
          { role: "user", content: "Turn 1 User", id: "u1", timestamp: "12:00 PM" },
          { role: "assistant", content: "Turn 1 Assistant", id: "a1", timestamp: "12:01 PM" },
        ],
      });
      const api = createMockApi();
      render(<ChatTab api={api} />);

      const editBtn = screen.getByTitle("Edit message");
      fireEvent.click(editBtn);

      const editTextarea = screen.getByDisplayValue("Turn 1 User");
      fireEvent.change(editTextarea, { target: { value: "Turn 1 User Edited" } });

      const saveBtn = screen.getByText("Save & Regenerate");
      await act(async () => {
        fireEvent.click(saveBtn);
      });

      expect(api.sendMessage).toHaveBeenCalled();
    });

    it("regenerates assistant message", async () => {
      useAppStore.setState({
        chatMessages: [
          { role: "user", content: "Hello", id: "u1", timestamp: "12:00 PM" },
          { role: "assistant", content: "Original Assistant", id: "a1", timestamp: "12:01 PM" },
        ],
      });
      const api = createMockApi();
      render(<ChatTab api={api} />);

      const regenBtn = screen.getByTitle("Regenerate assistant response");
      await act(async () => {
        fireEvent.click(regenBtn);
      });

      expect(api.sendMessage).toHaveBeenCalled();
    });

    it("allows changing system prompt preset from knobs panel", () => {
      const api = createMockApi();
      render(<ChatTab api={api} />);

      const knobsBtn = screen.getByTitle("Per-thread settings & system prompt presets");
      fireEvent.click(knobsBtn);

      const presetSelect = screen.getByLabelText("System Prompt Preset");
      fireEvent.change(presetSelect, { target: { value: "concise" } });

      expect(presetSelect).toHaveValue("concise");
    });

    it("renders keyboard-accessible thread item buttons in sidebar", () => {
      useAppStore.setState({
        threads: [
          { id: "thread-1", title: "Project Architecture", messages: [], createdAt: Date.now(), updatedAt: Date.now() },
        ],
        activeThreadId: "thread-1",
      });
      const api = createMockApi();
      render(<ChatTab api={api} />);

      const selectBtn = screen.getByRole("button", { name: "Select thread Project Architecture" });
      expect(selectBtn).toBeInTheDocument();

      const pinBtn = screen.getByRole("button", { name: "Pin thread Project Architecture" });
      expect(pinBtn).toBeInTheDocument();

      const renameBtn = screen.getByRole("button", { name: "Rename thread Project Architecture" });
      expect(renameBtn).toBeInTheDocument();

      const deleteBtn = screen.getByRole("button", { name: "Delete thread Project Architecture" });
      expect(deleteBtn).toBeInTheDocument();
    });

    it("toggles MCP tool selector popover overlay when clicking Select tools button", () => {
      useAppStore.setState({
        mcpServers: [
          {
            name: "calculator-mcp",
            slot: "calculator",
            enabled: true,
            connected: true,
            requires_confirmation: false,
            timeout_secs: 30,
            tool_count: 1,
            transport: { transport: "stdio", command: "calc", args: [], env: {} },
          },
        ],
      });
      const api = createMockApi();
      render(<ChatTab api={api} />);

      const selectToolsBtn = screen.getByRole("button", { name: "Select tools" });
      expect(selectToolsBtn).toHaveAttribute("aria-expanded", "false");

      fireEvent.click(selectToolsBtn);
      expect(selectToolsBtn).toHaveAttribute("aria-expanded", "true");
      expect(screen.getByText("Select MCP Tools")).toBeInTheDocument();
      expect(screen.getByText("calculator")).toBeInTheDocument();

      const checkbox = screen.getByRole("checkbox");
      expect(checkbox).not.toBeChecked();
      fireEvent.click(checkbox);
      expect(checkbox).toBeChecked();

      const closeBtn = screen.getByRole("button", { name: "Close tool selector" });
      fireEvent.click(closeBtn);
      expect(screen.queryByText("Select MCP Tools")).not.toBeInTheDocument();
    });
  });
});
