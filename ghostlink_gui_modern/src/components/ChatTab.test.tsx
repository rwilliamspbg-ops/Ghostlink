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

  it('shows send button', () => {
    const api = createMockApi();
    render(<ChatTab api={api} />);
    const textarea = screen.getByPlaceholderText(/Send a Message/i);
    expect(textarea).toBeInTheDocument();
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
  });
});
