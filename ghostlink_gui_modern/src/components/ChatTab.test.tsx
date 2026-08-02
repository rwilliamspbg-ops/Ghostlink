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
    const textarea = screen.getByPlaceholderText('Send a Message');
    expect(textarea).toBeInTheDocument();
  });

  it('allows typing a message', () => {
    const api = createMockApi();
    render(<ChatTab api={api} />);
    const textarea = screen.getByPlaceholderText('Send a Message');
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
      const textarea = screen.getByPlaceholderText('Send a Message');
      expect(textarea).toHaveValue('hello from voice');

      fireEvent.click(screen.getByLabelText('Stop voice input'));
      expect(instances[0].stop).toHaveBeenCalledOnce();
      expect(screen.getByLabelText('Start voice input')).toBeInTheDocument();
    });
  });
});
