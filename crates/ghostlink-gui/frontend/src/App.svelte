<script>
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  const navItems = [
    { id: 'Home', icon: '⌂', subtitle: 'Overview' },
    { id: 'Models', icon: '◈', subtitle: 'Catalog' },
    { id: 'Chat', icon: '✦', subtitle: 'Inference' },
    { id: 'Cluster', icon: '⎈', subtitle: 'Workers' },
    { id: 'Settings', icon: '⚙', subtitle: 'Config' },
    { id: 'Doctor', icon: '✚', subtitle: 'Diagnostics' },
  ];
  let activeTab = 'Home';
  let cards = [
    { label: 'Toolchain', value: 'Checking...' },
    { label: 'Python', value: 'Checking...' },
    { label: 'Local Config', value: 'Checking...' },
    { label: 'Doctor Artifact', value: 'Checking...' },
  ];

  let status = 'Loading...';
  let command = '';
  let output = 'No command executed yet.';
  let summary = 'Collecting startup snapshot...';
  let showOnboarding = false;
  let configPath = '';
  let configContent = '';
  let configLoaded = false;
  let doctorSummary = null;
  let modelRepo = 'sshleifer/tiny-gpt2';
  let modelFile = 'config.json';
  const DEFAULT_MODEL_FILE = 'config.json';
  let modelPresets = [];
  let modelCheck = null;
  let backendModels = [];
  let currentBackendModel = '';
  let backendModelStatus = { loadedModels: [], downloadingModels: {}, currentModel: '' };
  let selectedBackendModel = '';
  let modelActionMessage = '';
  let modelCatalogFilter = '';
  let hfQuery = 'text-generation';
  let hfModels = [];
  let hfLoading = false;
  let hfError = '';
  const modelDownloadCatalog = [
    { id: 'neural-chat', family: 'General', sizeHint: '~4-7GB', source: 'ollama' },
    { id: 'llama3.2:3b', family: 'General', sizeHint: '~2GB', source: 'ollama' },
    { id: 'mistral:7b', family: 'General', sizeHint: '~4GB', source: 'ollama' },
    { id: 'qwen2.5:7b', family: 'General', sizeHint: '~4GB', source: 'ollama' },
    { id: 'phi3:mini', family: 'Lightweight', sizeHint: '~2GB', source: 'ollama' },
    { id: 'nomic-embed-text', family: 'Embeddings', sizeHint: '~300MB', source: 'ollama' },
  ];
  let backendReachable = null;
  let ollamaReachable = null;
  let connectivityDetail = '';
  let chatPrompt = '';
  let chatModel = 'ghostlink-live-7b';
  let ollamaUrl = 'http://127.0.0.1:11434';
  let ollamaModel = 'neural-chat';
  let chatTemperature = 0.7;
  let chatMaxTokens = 256;
  let chatDistributed = true;
  let chatResult = null;
  let chatHistory = [];
  let chatResponseEl;
  let chatHistoryScrollEl;
  let chatAutoFollowResponse = true;
  let chatAutoFollowHistory = true;
  let clusterNodes = [];
  let clusterSummary = 'No live cluster snapshot loaded.';
  let workerDiscovery = [];
  let workerDiscoverySummary = 'Run discovery to list available workers.';
  let workerProbeHints = '';
  let workerProbeFull = false;
  let localNodeId = 'studio-local';
  let remoteNodeId = 'studio-remote';
  let flowTransport = 'tcp';
  let flowExecutionTokens = 64;
  let flowMicroBatch = 2;
  let selectedWorkerIds = [];
  let batchConnectResults = [];
  let workerTcpTargets = {};
  let workerTcpResults = {};
  let tcpProbeTimeoutMs = 500;
  let startNodeCount = 3;
  let startBasePort = 46000;
  let showAdvancedClusterButtons = false;
  let validationTier = 'fast';
  let validationReport = null;
  let snapshotHistory = [];
  let validationHistory = [];
  let profileName = 'local-default';
  let profilePath = './tmp/studio-profiles/local-default.json';
  let uiTheme = 'neon';
  let fontScale = 1;
  let reducedMotion = false;
  let highContrast = false;
  let busy = false;
  let initializing = true;

  const forceMockBridge = typeof window !== 'undefined' && new URLSearchParams(window.location.search).has('mock');
  const tauriRuntimeAvailable = typeof window !== 'undefined' && ('__TAURI_INTERNALS__' in window || '__TAURI_METADATA__' in window);
  const queryBackend = typeof window !== 'undefined' ? new URLSearchParams(window.location.search).get('backend') : null;
  const backendBaseUrl = (queryBackend || import.meta.env.VITE_GHOSTLINK_BACKEND_URL || 'http://127.0.0.1:9999').replace(/\/$/, '');
  const useMockBridge = forceMockBridge;
  const useHttpBridge = !tauriRuntimeAvailable && !useMockBridge;

  async function fetchJson(path) {
    const response = await fetch(`${backendBaseUrl}${path}`);
    if (!response.ok) {
      throw new Error(`HTTP ${response.status} while requesting ${path}`);
    }
    return response.json();
  }

  async function postJson(path, body) {
    const response = await fetch(`${backendBaseUrl}${path}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!response.ok) {
      throw new Error(`HTTP ${response.status} while requesting ${path}`);
    }
    return response.json();
  }

  async function mockInvoke(commandName, args = {}) {
    await new Promise((resolve) => setTimeout(resolve, 120));

    const defaultRun = {
      ok: true,
      command: commandName,
      stdout: 'Mock bridge command completed successfully.',
      stderr: '',
      exitCode: 0,
    };

    switch (commandName) {
      case 'studio_status':
        return {
          app: 'Ghostlink Studio',
          status: 'mock-preview-ready',
          repo_root: '/workspaces/Ghostlink',
        };
      case 'studio_snapshot':
        return {
          metrics: [
            { label: 'Toolchain', value: 'Rust 1.80 (stable)' },
            { label: 'Python', value: '3.12.2 + venv' },
            { label: 'Local Config', value: 'ghostlink.toml loaded' },
            { label: 'Doctor Artifact', value: 'report fresh (2m ago)' },
          ],
          summary: 'Snapshot complete: runtime + config surfaces healthy.',
          checksPassed: 9,
          checksWarn: 1,
        };
      case 'load_ghostlink_config':
        return {
          path: './ghostlink.toml',
          exists: true,
          content: '[flow]\ntransport = "tcp"\nexecution_tokens = 256\nmicro_batch = 8\n',
        };
      case 'save_ghostlink_config':
        return {
          path: './ghostlink.toml',
        };
      case 'run_doctor_with_json':
        return {
          path: './tmp/doctor-report.json',
          pass: 6,
          warn: 1,
          fail: 0,
          checks: [
            { status: 'PASS', area: 'runtime', name: 'toolchain', detail: 'Rust and Python runtimes detected.' },
            { status: 'PASS', area: 'network', name: 'backend', detail: 'Backend health endpoint reachable.' },
            { status: 'WARN', area: 'security', name: 'secrets', detail: 'Using local development token.', fix: 'Rotate token for production deploy.' },
          ],
        };
      case 'verify_hf_repo':
        return {
          ok: true,
          repo: args.repo,
          file: args.file,
          stdout: 'Repository and target file are accessible.',
          stderr: '',
        };
      case 'list_model_presets':
        return [
          { name: 'Fast Smoke', repo: 'sshleifer/tiny-gpt2', quant: 'int8', defaultFile: 'config.json' },
          { name: 'Balanced OSS', repo: 'mistralai/Mistral-7B-Instruct-v0.2', quant: 'q4', defaultFile: 'config.json' },
          { name: 'High Quality', repo: 'meta-llama/Meta-Llama-3-8B-Instruct', quant: 'q6', defaultFile: 'config.json' },
        ];
      case 'list_hf_models':
        return {
          query: String(args.query ?? ''),
          limit: Number(args.limit ?? 12),
          models: [
            { id: 'mistralai/Mistral-7B-Instruct-v0.2', pipeline_tag: 'text-generation', downloads: 1800000, likes: 4200, last_modified: '2026-06-18T00:00:00.000Z', private: false, gated: 'false', has_config: true, has_tokenizer: true },
            { id: 'meta-llama/Meta-Llama-3-8B-Instruct', pipeline_tag: 'text-generation', downloads: 2500000, likes: 9300, last_modified: '2026-05-28T00:00:00.000Z', private: false, gated: 'manual', has_config: true, has_tokenizer: true },
            { id: 'Qwen/Qwen2.5-7B-Instruct', pipeline_tag: 'text-generation', downloads: 1200000, likes: 3700, last_modified: '2026-06-07T00:00:00.000Z', private: false, gated: 'false', has_config: true, has_tokenizer: true },
          ],
        };
      case 'list_backend_models':
        return {
          models: [
            { name: 'neural-chat', size_gb: 7.0, type: 'LLM', quantization: 'Q4_K_M', status: 'Ready' },
            { name: 'mistral:7b', size_gb: 7.0, type: 'LLM', quantization: 'Q4_K_M', status: 'Ready' },
          ],
          current_model: 'neural-chat',
        };
      case 'list_backend_model_status':
        return {
          loaded_models: ['neural-chat', 'mistral:7b'],
          downloading_models: {},
          current_model: 'neural-chat',
        };
      case 'download_backend_model':
        return {
          status: 'ok',
          model_id: String(args.modelId ?? args.model_id ?? ''),
          downloaded: true,
          detail: 'mock download completed',
        };
      case 'delete_backend_model':
        return {
          status: 'ok',
          model: String(args.model ?? ''),
          deleted: true,
          detail: 'mock delete completed',
          current_model: 'neural-chat',
        };
      case 'load_backend_model':
        return {
          status: 'ok',
          model: String(args.model ?? ''),
          loaded: true,
        };
      case 'ollama_health':
        return {
          reachable: true,
          ollama_url: String(ollamaUrl),
          detail: 'mock bridge connected',
        };
      case 'load_flow_defaults':
        return {
          localId: 'studio-local',
          remoteId: 'studio-remote',
          executionTokens: 256,
          microBatch: 8,
          transport: 'tcp',
        };
      case 'discover_workers':
        return {
          summary: '3 workers discovered (2 reachable).',
          workers: [
            { id: 'studio-local', available: true, acceleration: 'gpu', workers: 4, probeMode: args.full ? 'full' : 'fast', systemMemoryGb: 32, gpuVramGb: 24 },
            { id: 'studio-remote', available: true, acceleration: 'gpu', workers: 3, probeMode: args.full ? 'full' : 'fast', systemMemoryGb: 64, gpuVramGb: 48 },
            { id: 'studio-edge', available: false, acceleration: 'cpu', workers: 2, probeMode: args.full ? 'full' : 'fast', systemMemoryGb: 16, gpuVramGb: 0, error: 'Connection timeout' },
          ],
        };
      case 'run_flow_between':
        return {
          ...defaultRun,
          command: `ghost-link flow ${args.localId} ${args.remoteId} 32 32 ${args.executionTokens} ${args.microBatch} ${args.transport}`,
          stdout: `Connected ${args.localId} -> ${args.remoteId} (${args.transport}, mb=${args.microBatch}).`,
        };
      case 'quick_tcp_probe':
        return {
          reachable: true,
          latencyMs: 2.7,
          error: null,
        };
      case 'cluster_preview':
        return {
          summary: args.full ? 'Deep cluster snapshot refreshed.' : 'Cluster snapshot refreshed.',
          nodes: [
            { id: 'studio-local', health: 'healthy', acceleration: 'gpu', workers: 4, systemMemoryGb: 32, gpuVramGb: 24 },
            { id: 'studio-remote', health: 'healthy', acceleration: 'gpu', workers: 3, systemMemoryGb: 64, gpuVramGb: 48 },
            { id: 'studio-edge', health: 'degraded', acceleration: 'cpu', workers: 2, systemMemoryGb: 16, gpuVramGb: 0 },
          ],
        };
      case 'chat_infer':
        return {
          backend: args.distributed ? 'distributed-backend' : 'local-loopback',
          model: args.model,
          response: 'Polished chat preview response from Ghostlink runtime.',
          trace: `tokens=${args.maxTokens} temp=${args.temperature}`,
        };
      case 'run_validation_tier':
        return {
          ok: true,
          tier: args.tier,
          summary: `${String(args.tier).toUpperCase()} validation completed with no regressions.`,
          steps: [
            { name: 'Doctor preflight', ok: true, durationMs: 420 },
            { name: 'Flow canary', ok: true, durationMs: 680 },
            { name: 'Docs consistency', ok: true, durationMs: 250 },
          ],
        };
      default:
        return defaultRun;
    }
  }

  async function httpInvoke(commandName, args = {}) {
    switch (commandName) {
      case 'studio_status': {
        const health = await fetchJson('/health');
        return {
          app: 'Ghostlink Studio',
          status: health.status === 'healthy' ? 'http-bridge-ready' : 'http-bridge-degraded',
          repo_root: 'served backend',
        };
      }
      case 'studio_snapshot': {
        const health = await fetchJson('/health');
        return {
          metrics: [
            { label: 'Backend Health', value: String(health.status ?? 'unknown') },
            { label: 'Model', value: String(health.current_model ?? 'unknown') },
            { label: 'Backend URL', value: String(health.backend_url ?? backendBaseUrl) },
            { label: 'Uptime', value: `${Number(health.uptime_s ?? 0)}s` },
          ],
          summary: 'Snapshot loaded from HTTP backend bridge.',
          checksPassed: health.status === 'healthy' ? 4 : 2,
          checksWarn: health.status === 'healthy' ? 0 : 2,
        };
      }
      case 'load_ghostlink_config':
        return {
          path: 'browser-http-bridge',
          exists: false,
          content: '# Config editing requires Tauri runtime.\n',
        };
      case 'save_ghostlink_config':
        return { path: 'browser-http-bridge' };
      case 'list_model_presets':
        return [
          { name: 'Tiny GPT-2 (smoke)', repo: 'sshleifer/tiny-gpt2', quant: 'int8', defaultFile: 'config.json' },
          { name: 'Mistral 7B', repo: 'mistralai/Mistral-7B-v0.1', quant: 'q4', defaultFile: 'config.json' },
          { name: 'Llama 3 8B', repo: 'meta-llama/Meta-Llama-3-8B-Instruct', quant: 'q4', defaultFile: 'config.json' },
        ];
      case 'list_hf_models': {
        const q = String(args.query ?? '').trim();
        const l = Number(args.limit ?? 12);
        const url = new URL('https://huggingface.co/api/models');
        if (q) {
          url.searchParams.set('search', q);
        }
        url.searchParams.set('limit', String(Number.isFinite(l) ? Math.max(1, Math.min(50, l)) : 12));
        url.searchParams.set('sort', 'downloads');
        url.searchParams.set('direction', '-1');
        url.searchParams.set('full', 'true');

        const response = await fetch(url.toString());
        if (!response.ok) {
          throw new Error(`HTTP ${response.status} while requesting Hugging Face models`);
        }
        const data = await response.json();
        const models = Array.isArray(data)
          ? data.map((entry) => {
              const siblings = Array.isArray(entry?.siblings) ? entry.siblings : [];
              const hasConfig = siblings.some((item) => item?.rfilename === 'config.json');
              const hasTokenizer = siblings.some((item) => item?.rfilename === 'tokenizer.json' || item?.rfilename === 'tokenizer_config.json');
              return {
                id: String(entry?.id ?? ''),
                pipeline_tag: String(entry?.pipeline_tag ?? 'unknown'),
                downloads: Number(entry?.downloads ?? 0),
                likes: Number(entry?.likes ?? 0),
                last_modified: String(entry?.lastModified ?? ''),
                private: Boolean(entry?.private),
                gated: String(entry?.gated ?? 'false'),
                has_config: hasConfig,
                has_tokenizer: hasTokenizer,
              };
            })
          : [];

        return { query: q, limit: l, models };
      }
      case 'list_backend_models':
        return fetchJson('/api/models');
      case 'list_backend_model_status':
        return fetchJson('/api/models/status');
      case 'download_backend_model':
        return postJson('/api/models/download', {
          model_id: String(args.modelId ?? args.model_id ?? ''),
        });
      case 'delete_backend_model':
        return postJson('/api/models/delete', {
          model: String(args.model ?? ''),
        });
      case 'load_backend_model':
        return postJson('/api/models/load', {
          model: String(args.model ?? ''),
        });
      case 'ollama_health':
        return fetchJson('/api/ollama/health');
      case 'load_flow_defaults':
        return {
          localId: 'studio-local',
          remoteId: 'studio-remote',
          executionTokens: 256,
          microBatch: 8,
          transport: 'tcp',
        };
      case 'discover_workers': {
        const workersPayload = await fetchJson('/api/workers');
        const workers = Array.isArray(workersPayload.workers)
          ? workersPayload.workers.map((worker) => ({
              id: String(worker.id ?? 'unknown'),
              available: String(worker.status ?? '').toLowerCase() !== 'disconnected',
              workers: Number(worker.threads ?? 0),
              systemMemoryGb: 0,
              gpuVramGb: 0,
              acceleration: 'runtime',
              health: String(worker.status ?? 'unknown').toLowerCase(),
              probeMode: args.full ? 'full' : 'http',
              error: null,
            }))
          : [];
        return {
          workers,
          summary: `${workers.filter((item) => item.available).length} of ${workers.length} workers reachable`,
        };
      }
      case 'cluster_preview': {
        const workersPayload = await fetchJson('/api/workers');
        const nodes = Array.isArray(workersPayload.workers)
          ? workersPayload.workers.map((worker) => ({
              id: String(worker.id ?? 'unknown'),
              acceleration: 'runtime',
              workers: Number(worker.threads ?? 0),
              systemMemoryGb: 0,
              gpuVramGb: 0,
              health: String(worker.status ?? 'unknown').toLowerCase() === 'connected' ? 'healthy' : 'degraded',
            }))
          : [];
        return {
          nodes,
          summary: `HTTP snapshot loaded (${nodes.length} nodes).`,
        };
      }
      case 'run_flow_between': {
        const result = await postJson('/api/workers/connect', {});
        const connected = Number(result.connected ?? 0);
        const total = Number(result.total ?? 0);
        return {
          ok: true,
          command: `POST /api/workers/connect`,
          stdout: `Connected ${connected}/${total} worker(s) via backend API.`,
          stderr: '',
          exitCode: 0,
        };
      }
      case 'quick_tcp_probe':
        return {
          reachable: false,
          latencyMs: null,
          error: 'Quick TCP probe is only available in Tauri mode.',
        };
      case 'chat_infer': {
        const payload = {
          message: String(args.prompt ?? ''),
          model: String(args.model ?? ollamaModel),
          ollama_url: String(ollamaUrl ?? ''),
          temperature: Number(args.temperature ?? 0.7),
            max_tokens: Number(args.maxTokens ?? args.max_tokens ?? 256),
        };
        const result = await postJson('/api/inference/chat', payload);
        return {
          backend: 'http-backend-api',
          model: args.model,
          response: String(result.response ?? ''),
          trace: `request_id=${result.request_id ?? 'n/a'} exec_tokens=${result.exec_tokens ?? 'n/a'} micro_batch=${result.exec_micro_batch ?? 'n/a'}`,
        };
      }
      default:
        throw new Error(`Command '${commandName}' is only available in Tauri mode.`);
    }
  }

  async function bridgeInvoke(commandName, args = {}) {
    if (tauriRuntimeAvailable) {
      return invoke(commandName, args);
    }
    if (useHttpBridge) {
      return httpInvoke(commandName, args);
    }
    if (useMockBridge) {
      return mockInvoke(commandName, args);
    }
    throw new Error('No bridge is available for this runtime.');
  }

  function applyVisualPreferences() {
    document.body.dataset.theme = uiTheme;
    document.body.style.setProperty('--studio-font-scale', String(fontScale));
    document.body.classList.toggle('reduced-motion', reducedMotion);
    document.body.classList.toggle('high-contrast', highContrast);
  }

  function persistPreferences() {
    const prefs = {
      uiTheme,
      fontScale,
      reducedMotion,
      highContrast,
      ollamaUrl,
      ollamaModel,
      chatHistory,
      workerProbeHints,
      workerProbeFull,
      localNodeId,
      remoteNodeId,
      flowTransport,
      flowExecutionTokens,
      flowMicroBatch,
      startNodeCount,
      startBasePort,
      showAdvancedClusterButtons,
      tcpProbeTimeoutMs,
    };
    localStorage.setItem('ghostlink-studio-prefs-v1', JSON.stringify(prefs));
  }

  function loadPreferences() {
    const raw = localStorage.getItem('ghostlink-studio-prefs-v1');
    if (!raw) {
      showOnboarding = true;
      return;
    }

    try {
      const prefs = JSON.parse(raw);
      uiTheme = prefs.uiTheme ?? 'neon';
      fontScale = Number(prefs.fontScale ?? 1);
      reducedMotion = Boolean(prefs.reducedMotion);
      highContrast = Boolean(prefs.highContrast);
      ollamaUrl = String(prefs.ollamaUrl ?? ollamaUrl);
      ollamaModel = String(prefs.ollamaModel ?? ollamaModel);
      chatHistory = Array.isArray(prefs.chatHistory) ? prefs.chatHistory.slice(0, 12) : [];
      workerProbeHints = String(prefs.workerProbeHints ?? workerProbeHints);
      workerProbeFull = Boolean(prefs.workerProbeFull);
      localNodeId = String(prefs.localNodeId ?? localNodeId);
      remoteNodeId = String(prefs.remoteNodeId ?? remoteNodeId);
      flowTransport = String(prefs.flowTransport ?? flowTransport).toLowerCase();
      flowExecutionTokens = Number(prefs.flowExecutionTokens ?? flowExecutionTokens);
      flowMicroBatch = Number(prefs.flowMicroBatch ?? flowMicroBatch);
      startNodeCount = Number(prefs.startNodeCount ?? startNodeCount);
      startBasePort = Number(prefs.startBasePort ?? startBasePort);
      showAdvancedClusterButtons = Boolean(prefs.showAdvancedClusterButtons);
      tcpProbeTimeoutMs = Number(prefs.tcpProbeTimeoutMs ?? tcpProbeTimeoutMs);
    } catch {
      showOnboarding = true;
    }
  }

  function closeOnboarding() {
    showOnboarding = false;
    persistPreferences();
  }

  function resetPreferences() {
    uiTheme = 'neon';
    fontScale = 1;
    reducedMotion = false;
    highContrast = false;
    ollamaUrl = 'http://127.0.0.1:11434';
    ollamaModel = 'neural-chat';
    applyVisualPreferences();
    persistPreferences();
  }

  async function exportProfile() {
    busy = true;
    try {
      const result = await bridgeInvoke('export_studio_profile', {
        profileName,
        uiTheme,
        fontScale: Number(fontScale),
        reducedMotion,
        highContrast,
        modelRepo,
        modelFile,
        chatModel,
        chatDistributed,
        ollamaUrl,
        ollamaModel,
        configContent,
        workerProbeHints,
        workerProbeFull,
        localNodeId,
        remoteNodeId,
        flowTransport,
        flowExecutionTokens: Number(flowExecutionTokens),
        flowMicroBatch: Number(flowMicroBatch),
        startNodeCount: Number(startNodeCount),
        startBasePort: Number(startBasePort),
        showAdvancedClusterButtons,
      });
      profilePath = result.profilePath;
      status = 'Studio profile exported';
      output = `Profile written to ${result.profilePath}`;
    } catch (err) {
      status = 'Profile export failed';
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function importProfile() {
    busy = true;
    try {
      const profile = await bridgeInvoke('import_studio_profile', { profilePath });
      profileName = profile.profileName;
      uiTheme = profile.uiTheme;
      fontScale = Number(profile.fontScale);
      reducedMotion = Boolean(profile.reducedMotion);
      highContrast = Boolean(profile.highContrast);
      modelRepo = profile.modelRepo;
      modelFile = normalizeModelFile(profile.modelFile);
      chatModel = profile.chatModel;
      chatDistributed = Boolean(profile.chatDistributed);
      ollamaUrl = String(flowArg(ollamaUrl, profile.ollamaUrl, profile.ollama_url));
      ollamaModel = String(flowArg(ollamaModel, profile.ollamaModel, profile.ollama_model));
      configContent = profile.configContent;
      workerProbeHints = String(flowArg(workerProbeHints, profile.workerProbeHints, profile.worker_probe_hints));
      workerProbeFull = Boolean(flowArg(workerProbeFull, profile.workerProbeFull, profile.worker_probe_full));
      localNodeId = String(flowArg(localNodeId, profile.localNodeId, profile.local_node_id));
      remoteNodeId = String(flowArg(remoteNodeId, profile.remoteNodeId, profile.remote_node_id));
      flowTransport = String(flowArg(flowTransport, profile.flowTransport, profile.flow_transport)).toLowerCase();
      flowExecutionTokens = Number(flowArg(flowExecutionTokens, profile.flowExecutionTokens, profile.flow_execution_tokens));
      flowMicroBatch = Number(flowArg(flowMicroBatch, profile.flowMicroBatch, profile.flow_micro_batch));
      startNodeCount = Number(flowArg(startNodeCount, profile.startNodeCount, profile.start_node_count));
      startBasePort = Number(flowArg(startBasePort, profile.startBasePort, profile.start_base_port));
      showAdvancedClusterButtons = Boolean(flowArg(showAdvancedClusterButtons, profile.showAdvancedClusterButtons, profile.show_advanced_cluster_buttons));
      configLoaded = true;
      applyVisualPreferences();
      persistPreferences();
      status = 'Studio profile imported';
      output = `Applied profile ${profile.profileName}`;
    } catch (err) {
      status = 'Profile import failed';
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function loadSnapshot() {
    const snapshot = await bridgeInvoke('studio_snapshot');
    cards = snapshot.metrics.map((metric) => ({ label: metric.label, value: metric.value }));
    summary = snapshot.summary;

    const passed = Number(snapshot.checksPassed ?? snapshot.checks_passed ?? 0);
    const warn = Number(snapshot.checksWarn ?? snapshot.checks_warn ?? 0);
    const total = Math.max(1, passed + warn);
    snapshotHistory = [
      {
        time: new Date().toLocaleTimeString(),
        passed,
        warn,
        total,
        passPct: Math.round((passed / total) * 100),
      },
      ...snapshotHistory,
    ].slice(0, 10);
  }

  async function run(action, args = {}) {
    busy = true;
    try {
      const result = await bridgeInvoke(action, args);
      status = result.ok ? 'Command succeeded' : 'Command failed';
      command = result.command;
      output = [result.stdout?.trim(), result.stderr?.trim()].filter(Boolean).join('\n\n');
      if (!output) {
        output = 'Command completed with no output.';
      }

      if (action === 'run_probe' && result.ok) {
        await loadSnapshot();
      }

      if (action === 'run_doctor' && result.ok) {
        await loadSnapshot();
      }
    } catch (err) {
      status = 'Command invocation error';
      command = action;
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function loadConfig() {
    busy = true;
    try {
      const cfg = await bridgeInvoke('load_ghostlink_config');
      configPath = cfg.path;
      configContent = cfg.content;
      configLoaded = true;
      status = cfg.exists ? 'Loaded local config' : 'Loaded example config (local missing)';
    } catch (err) {
      status = 'Config load failed';
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function saveConfig() {
    busy = true;
    try {
      const cfg = await bridgeInvoke('save_ghostlink_config', { content: configContent });
      configPath = cfg.path;
      status = 'Config saved';
      output = `Saved ${cfg.path}`;
      await loadSnapshot();
    } catch (err) {
      status = 'Config save failed';
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function runDoctorJson(strict) {
    busy = true;
    doctorSummary = null;
    try {
      const report = await bridgeInvoke('run_doctor_with_json', { strict });
      doctorSummary = report;
      status = strict ? 'Doctor strict report generated' : 'Doctor report generated';
      output = `Doctor JSON: ${report.path}`;
      await loadSnapshot();
    } catch (err) {
      status = 'Doctor run failed';
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function verifyModel() {
    busy = true;
    modelCheck = null;
    try {
      const requestedFile = normalizeModelFile(modelFile);
      modelFile = requestedFile;
      const result = await bridgeInvoke('verify_hf_repo', {
        repo: modelRepo,
        file: requestedFile,
      });
      modelCheck = result;
      status = result.ok ? 'Model verification passed' : 'Model verification failed';
      output = [result.stdout?.trim(), result.stderr?.trim()].filter(Boolean).join('\n\n');
    } catch (err) {
      status = 'Model verification failed';
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function loadModelPresets() {
    const presets = await bridgeInvoke('list_model_presets');
    modelPresets = presets;
  }

  async function refreshModelStatus() {
    try {
      const result = await bridgeInvoke('list_backend_model_status');
      backendModelStatus = {
        loadedModels: Array.isArray(result?.loaded_models) ? result.loaded_models : [],
        downloadingModels: result?.downloading_models ?? {},
        currentModel: String(result?.current_model ?? ''),
      };
    } catch {
      backendModelStatus = { loadedModels: [], downloadingModels: {}, currentModel: '' };
    }
  }

  async function loadHfModels() {
    hfLoading = true;
    hfError = '';
    try {
      const result = await bridgeInvoke('list_hf_models', {
        query: hfQuery,
        limit: 12,
      });
      hfModels = Array.isArray(result?.models) ? result.models : [];
    } catch (err) {
      hfError = String(err);
      hfModels = [];
    } finally {
      hfLoading = false;
    }
  }

  async function refreshConnectivity() {
    if (useMockBridge) {
      backendReachable = true;
      ollamaReachable = true;
      connectivityDetail = 'mock bridge';
      return;
    }

    try {
      await fetchJson('/health');
      backendReachable = true;
    } catch (err) {
      backendReachable = false;
      ollamaReachable = null;
      connectivityDetail = String(err);
      return;
    }

    try {
      const ollama = await bridgeInvoke('ollama_health');
      ollamaReachable = Boolean(ollama.reachable);
      connectivityDetail = String(ollama.detail ?? '');
    } catch (err) {
      ollamaReachable = false;
      connectivityDetail = String(err);
    }
  }

  function applyBackendModels(result) {
    const models = Array.isArray(result?.models) ? result.models : [];
    backendModels = models;
    currentBackendModel = String(result?.current_model ?? '');
    if (!selectedBackendModel || !models.some((entry) => entry?.name === selectedBackendModel)) {
      selectedBackendModel = String(models[0]?.name ?? currentBackendModel ?? '');
    }
  }

  function filteredModelCatalog() {
    const needle = modelCatalogFilter.trim().toLowerCase();
    if (!needle) {
      return modelDownloadCatalog;
    }
    return modelDownloadCatalog.filter((entry) => {
      const haystack = `${entry.id} ${entry.family} ${entry.sizeHint} ${entry.source}`.toLowerCase();
      return haystack.includes(needle);
    });
  }

  function chooseCatalogModel(modelId) {
    selectedBackendModel = String(modelId ?? '').trim();
    if (selectedBackendModel) {
      ollamaModel = selectedBackendModel;
      modelActionMessage = `Selected catalog model ${selectedBackendModel}`;
    }
  }

  function useHfModel(modelId) {
    const repo = String(modelId ?? '').trim();
    if (!repo) {
      return;
    }
    modelRepo = repo;
    modelFile = DEFAULT_MODEL_FILE;
    modelActionMessage = `Selected Hugging Face repo ${repo}`;
  }

  async function verifyHfCandidate(modelId) {
    useHfModel(modelId);
    await verifyModel();
  }

  async function loadBackendModels() {
    await refreshConnectivity();
    if (!backendReachable) {
      throw new Error(`Backend unavailable at ${backendBaseUrl}`);
    }
    const result = await bridgeInvoke('list_backend_models');
    applyBackendModels(result);
    await refreshModelStatus();
  }

  async function downloadSelectedModel() {
    if (!selectedBackendModel.trim()) {
      modelActionMessage = 'Select a model first.';
      return;
    }
    busy = true;
    try {
      await refreshConnectivity();
      if (!backendReachable) {
        throw new Error(`Backend unavailable at ${backendBaseUrl}`);
      }
      if (!ollamaReachable) {
        throw new Error(`Ollama unavailable at ${ollamaUrl}`);
      }
      const result = await bridgeInvoke('download_backend_model', {
        modelId: selectedBackendModel,
        model_id: selectedBackendModel,
      });
      modelActionMessage = String(result.detail ?? `Download requested for ${selectedBackendModel}`);
      status = `Model download: ${result.status ?? 'unknown'}`;
      await loadBackendModels();
    } catch (err) {
      status = 'Model download failed';
      modelActionMessage = String(err);
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function downloadCatalogModel(modelId) {
    selectedBackendModel = String(modelId ?? '').trim();
    await downloadSelectedModel();
  }

  async function loadSelectedModel() {
    if (!selectedBackendModel.trim()) {
      modelActionMessage = 'Select a model first.';
      return;
    }
    busy = true;
    try {
      await refreshConnectivity();
      if (!backendReachable) {
        throw new Error(`Backend unavailable at ${backendBaseUrl}`);
      }
      if (!ollamaReachable) {
        throw new Error(`Ollama unavailable at ${ollamaUrl}`);
      }
      const result = await bridgeInvoke('load_backend_model', {
        model: selectedBackendModel,
      });
      modelActionMessage = result.loaded
        ? `Loaded ${result.model ?? selectedBackendModel}`
        : String(result.error ?? 'Model load failed');
      status = result.loaded ? 'Model switched' : 'Model load failed';
      await loadBackendModels();
    } catch (err) {
      status = 'Model load failed';
      modelActionMessage = String(err);
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function setActiveModel(modelName) {
    selectedBackendModel = String(modelName ?? '').trim();
    await loadSelectedModel();
  }

  async function deleteSelectedModel() {
    if (!selectedBackendModel.trim()) {
      modelActionMessage = 'Select a model first.';
      return;
    }

    busy = true;
    try {
      await refreshConnectivity();
      if (!backendReachable) {
        throw new Error(`Backend unavailable at ${backendBaseUrl}`);
      }
      if (!ollamaReachable) {
        throw new Error(`Ollama unavailable at ${ollamaUrl}`);
      }

      const target = selectedBackendModel.trim();
      const result = await bridgeInvoke('delete_backend_model', {
        model: target,
      });
      modelActionMessage = String(result.detail ?? `Deleted ${target}`);
      status = result.deleted ? 'Model deleted' : 'Model delete failed';
      await loadBackendModels();
    } catch (err) {
      status = 'Model delete failed';
      modelActionMessage = String(err);
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function deleteModel(modelName) {
    selectedBackendModel = String(modelName ?? '').trim();
    await deleteSelectedModel();
  }

  function applyPreset(indexValue) {
    const index = Number(indexValue);
    if (!Number.isInteger(index) || index < 0 || index >= modelPresets.length) {
      return;
    }

    const preset = modelPresets[index];
    modelRepo = preset.repo;
    modelFile = normalizeModelFile(preset.defaultFile);
  }

  function normalizeModelFile(value) {
    const normalized = String(value ?? '').trim();
    return normalized || DEFAULT_MODEL_FILE;
  }

  function isNearBottom(element, threshold = 28) {
    if (!element) {
      return true;
    }
    const remaining = element.scrollHeight - (element.scrollTop + element.clientHeight);
    return remaining <= threshold;
  }

  function handleChatResponseScroll() {
    chatAutoFollowResponse = isNearBottom(chatResponseEl);
  }

  function handleChatHistoryScroll() {
    chatAutoFollowHistory = isNearBottom(chatHistoryScrollEl);
  }

  function autoScrollChatPanels() {
    if (chatAutoFollowResponse && chatResponseEl) {
      chatResponseEl.scrollTop = chatResponseEl.scrollHeight;
    }
    if (chatAutoFollowHistory && chatHistoryScrollEl) {
      chatHistoryScrollEl.scrollTop = chatHistoryScrollEl.scrollHeight;
    }
  }

  function parseNodeHints(raw) {
    return String(raw)
      .split(',')
      .map((part) => part.trim())
      .filter(Boolean);
  }

  function flowArg(defaultValue, camelValue, snakeValue) {
    if (camelValue !== undefined && camelValue !== null && `${camelValue}`.trim() !== '') {
      return camelValue;
    }
    if (snakeValue !== undefined && snakeValue !== null && `${snakeValue}`.trim() !== '') {
      return snakeValue;
    }
    return defaultValue;
  }

  async function loadFlowDefaults() {
    try {
      const defaults = await bridgeInvoke('load_flow_defaults');
      localNodeId = String(flowArg(localNodeId, defaults.localId, defaults.local_id));
      remoteNodeId = String(flowArg(remoteNodeId, defaults.remoteId, defaults.remote_id));
      flowExecutionTokens = Number(flowArg(flowExecutionTokens, defaults.executionTokens, defaults.execution_tokens));
      flowMicroBatch = Number(flowArg(flowMicroBatch, defaults.microBatch, defaults.micro_batch));
      flowTransport = String(flowArg(flowTransport, defaults.transport, defaults.transport)).toLowerCase();
      workerProbeHints = [localNodeId, remoteNodeId].filter(Boolean).join(', ');
    } catch {
      workerProbeHints = [localNodeId, remoteNodeId].join(', ');
    }
  }

  async function discoverWorkers() {
    busy = true;
    try {
      const nodeIds = parseNodeHints(workerProbeHints);
      const result = await bridgeInvoke('discover_workers', { nodeIds, full: workerProbeFull });
      workerDiscovery = Array.isArray(result.workers) ? result.workers : [];
      workerDiscoverySummary = result.summary ?? 'Worker discovery completed.';

      const nextTargets = { ...workerTcpTargets };
      for (const worker of workerDiscovery) {
        if (!nextTargets[worker.id]) {
          nextTargets[worker.id] = { host: '127.0.0.1', port: Number(startBasePort) };
        }
      }
      workerTcpTargets = nextTargets;

      const knownIds = new Set(workerDiscovery.map((item) => item.id));
      selectedWorkerIds = selectedWorkerIds.filter((id) => knownIds.has(id));
      batchConnectResults = batchConnectResults.filter((item) => knownIds.has(item.workerId));

      const firstHealthy = workerDiscovery.find((item) => item.available);
      if (firstHealthy && !nodeIds.includes(firstHealthy.id)) {
        workerProbeHints = [workerProbeHints, firstHealthy.id].filter(Boolean).join(', ');
      }

      status = workerDiscoverySummary;
      command = `discover_workers (${workerProbeFull ? 'full' : 'fast'})`;
      output = workerDiscovery
        .map((item) => {
          if (item.available) {
            return `${item.id}: reachable, workers=${item.workers}, acceleration=${item.acceleration}`;
          }
          return `${item.id}: unreachable${item.error ? ` (${item.error})` : ''}`;
        })
        .join('\n');
    } catch (err) {
      status = 'Worker discovery failed';
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function connectFlow() {
    busy = true;
    try {
      const result = await bridgeInvoke('run_flow_between', {
        localId: localNodeId,
        remoteId: remoteNodeId,
        executionTokens: Number(flowExecutionTokens),
        microBatch: Number(flowMicroBatch),
        transport: flowTransport,
      });
      status = result.ok ? `Flow connected: ${localNodeId} -> ${remoteNodeId}` : 'Flow connection failed';
      command = result.command;
      output = [result.stdout?.trim(), result.stderr?.trim()].filter(Boolean).join('\n\n');
      if (!output) {
        output = 'Flow command completed with no output.';
      }
      await refreshCluster(false);
    } catch (err) {
      status = 'Flow connection failed';
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function connectToWorker(workerId) {
    remoteNodeId = workerId;
    await connectFlow();
  }

  function ensureTcpTarget(workerId) {
    const current = workerTcpTargets[workerId];
    if (current && current.host && Number(current.port) > 0) {
      return current;
    }

    const target = {
      host: current?.host ?? '127.0.0.1',
      port: Number(current?.port ?? startBasePort ?? 46000),
    };
    workerTcpTargets = { ...workerTcpTargets, [workerId]: target };
    return target;
  }

  function tcpTargetFor(workerId) {
    const current = workerTcpTargets[workerId];
    return {
      host: current?.host ?? '127.0.0.1',
      port: Number(current?.port ?? startBasePort ?? 46000),
    };
  }

  function updateTcpTarget(workerId, key, value) {
    const existing = ensureTcpTarget(workerId);
    const next = {
      ...existing,
      [key]: key === 'port' ? Number(value) : value,
    };
    workerTcpTargets = { ...workerTcpTargets, [workerId]: next };
  }

  function isWorkerSelected(workerId) {
    return selectedWorkerIds.includes(workerId);
  }

  function toggleWorkerSelection(workerId, checked) {
    if (checked) {
      selectedWorkerIds = Array.from(new Set([...selectedWorkerIds, workerId]));
      return;
    }
    selectedWorkerIds = selectedWorkerIds.filter((id) => id !== workerId);
  }

  function selectAllReachableWorkers() {
    selectedWorkerIds = workerDiscovery.filter((item) => item.available).map((item) => item.id);
  }

  function clearWorkerSelection() {
    selectedWorkerIds = [];
  }

  async function testWorkerTcp(workerId) {
    const target = ensureTcpTarget(workerId);
    busy = true;
    try {
      const result = await bridgeInvoke('quick_tcp_probe', {
        host: target.host,
        port: Number(target.port),
        timeoutMs: Number(tcpProbeTimeoutMs),
      });
      workerTcpResults = {
        ...workerTcpResults,
        [workerId]: {
          ...result,
          testedAt: new Date().toLocaleTimeString(),
        },
      };
      status = result.reachable ? `TCP reachable: ${target.host}:${target.port}` : `TCP unreachable: ${target.host}:${target.port}`;
      output = result.reachable
        ? `latency=${result.latencyMs ?? result.latency_ms ?? 'n/a'} ms`
        : String(result.error ?? 'connection failed');
    } catch (err) {
      status = `TCP probe failed for ${workerId}`;
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function connectAllReachableWorkers() {
    busy = true;
    batchConnectResults = [];

    try {
      const selectedSet = new Set(selectedWorkerIds);
      const targets = workerDiscovery
        .filter((worker) => worker.available)
        .filter((worker) => (selectedSet.size === 0 ? true : selectedSet.has(worker.id)))
        .filter((worker) => worker.id !== localNodeId);

      if (targets.length === 0) {
        status = 'No reachable workers selected';
        output = 'Select workers or run discovery first.';
        return;
      }

      const results = [];
      for (const worker of targets) {
        const started = Date.now();
        try {
          const result = await bridgeInvoke('run_flow_between', {
            localId: localNodeId,
            remoteId: worker.id,
            executionTokens: Number(flowExecutionTokens),
            microBatch: Number(flowMicroBatch),
            transport: flowTransport,
          });
          results.push({
            workerId: worker.id,
            ok: Boolean(result.ok),
            exitCode: result.exitCode ?? result.exit_code ?? null,
            durationMs: Date.now() - started,
            command: result.command,
            error: result.ok ? null : [result.stderr?.trim(), result.stdout?.trim()].filter(Boolean).join(' | '),
          });
        } catch (err) {
          results.push({
            workerId: worker.id,
            ok: false,
            exitCode: null,
            durationMs: Date.now() - started,
            command: 'run_flow_between',
            error: String(err),
          });
        }
      }

      batchConnectResults = results;
      const passed = results.filter((item) => item.ok).length;
      const failed = results.length - passed;
      status = `Batch connect complete: ${passed} ok, ${failed} failed`;
      output = results
        .map((item) => `${item.workerId}: ${item.ok ? 'OK' : 'FAIL'} (${item.durationMs} ms)${item.error ? ` - ${item.error}` : ''}`)
        .join('\n');
      await refreshCluster(false);
    } finally {
      busy = false;
    }
  }

  async function refreshCluster(full = false) {
    busy = true;
    try {
      const snapshot = await bridgeInvoke('cluster_preview', { nodeId: 'studio-local', full });
      clusterNodes = snapshot.nodes;
      clusterSummary = snapshot.summary;
      status = 'Cluster snapshot refreshed';
    } catch (err) {
      status = 'Cluster snapshot failed';
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function runChat() {
    busy = true;
    chatResult = null;
    try {
      await refreshConnectivity();
      if (!backendReachable) {
        throw new Error(`Backend unavailable at ${backendBaseUrl}`);
      }
      const result = await bridgeInvoke('chat_infer', {
        prompt: chatPrompt,
        model: chatModel,
        temperature: Number(chatTemperature),
        maxTokens: Number(chatMaxTokens),
        max_tokens: Number(chatMaxTokens),
        distributed: chatDistributed,
        ollamaUrl,
        ollama_url: ollamaUrl,
        ollamaModel,
        ollama_model: ollamaModel,
      });
      chatResult = result;
      status = `Chat response generated via ${result.backend}`;
      command = 'chat_infer';
      output = result.trace;
      chatHistory = [
        ...chatHistory,
        {
          prompt: chatPrompt,
          response: result.response,
          model: result.model,
          backend: result.backend,
        },
      ].slice(-12);
      persistPreferences();
      await tick();
      autoScrollChatPanels();
    } catch (err) {
      status = 'Chat generation failed';
      output = String(err);
    } finally {
      busy = false;
    }
  }

  async function runValidation() {
    busy = true;
    validationReport = null;
    try {
      const report = await bridgeInvoke('run_validation_tier', { tier: validationTier });
      validationReport = report;
      status = report.ok ? 'Validation completed successfully' : 'Validation found failures';
      output = report.summary;

      const steps = Array.isArray(report.steps) ? report.steps : [];
      const durationMs = steps.reduce((sum, step) => {
        const value = Number(step.durationMs ?? step.duration_ms ?? 0);
        return sum + (Number.isFinite(value) ? value : 0);
      }, 0);
      validationHistory = [
        {
          time: new Date().toLocaleTimeString(),
          tier: String(report.tier ?? validationTier).toUpperCase(),
          ok: Boolean(report.ok),
          durationMs,
        },
        ...validationHistory,
      ].slice(0, 10);
    } catch (err) {
      status = 'Validation run failed';
      output = String(err);
    } finally {
      busy = false;
    }
  }

  onMount(async () => {
    loadPreferences();
    applyVisualPreferences();
    try {
      const studio = await bridgeInvoke('studio_status');
      status = `${studio.app}: ${studio.status}`;
      output = `Repo root: ${studio.repo_root}`;
      await loadSnapshot();
      await loadConfig();
      await loadFlowDefaults();
      await loadModelPresets();
      await loadHfModels();
      await refreshConnectivity();
      await loadBackendModels();
      await refreshModelStatus();
      await refreshCluster(false);
      await discoverWorkers();
    } catch (err) {
      status = 'Studio bridge unavailable';
      output = String(err);
    } finally {
      initializing = false;
    }
  });

  $: reachableWorkerCount = workerDiscovery.filter((item) => item.available).length;
  $: selectedWorkerCount = selectedWorkerIds.length;
  $: selectedReachableWorkerCount = workerDiscovery.filter((item) => item.available && selectedWorkerIds.includes(item.id)).length;

  $: applyVisualPreferences();
  $: persistPreferences();
</script>

<div class="studio-shell">
  <aside class="sidebar">
    <div class="brand">
      <span class="brand-mark">GL</span>
      <div>
        <strong>Ghostlink Studio</strong>
        <small>Fabric Control Plane</small>
      </div>
    </div>
    {#if useHttpBridge}
      <div class="preview-banner">Browser mode (HTTP bridge)</div>
    {:else if useMockBridge}
      <div class="preview-banner">Preview mode (mock bridge)</div>
    {/if}
    {#each navItems as item}
      <button class="nav-item" class:active={item.id === activeTab} on:click={() => (activeTab = item.id)}>
        <span class="nav-icon">{item.icon}</span>
        <span>
          <strong>{item.id}</strong>
          <small>{item.subtitle}</small>
        </span>
      </button>
    {/each}
  </aside>

  <main class="dashboard">
    <div class="actions">
      <span>Backend: {backendReachable === null ? 'unknown' : backendReachable ? 'online' : 'offline'}</span>
      <span>Ollama: {ollamaReachable === null ? 'unknown' : ollamaReachable ? 'online' : 'offline'}</span>
      <button on:click={refreshConnectivity} disabled={busy}>Recheck Connectivity</button>
    </div>
    {#if connectivityDetail}
      <p>{connectivityDetail}</p>
    {/if}

    {#if busy}
      <div class="busy-banner"><span class="busy-dot"></span>Working... {status}</div>
    {/if}

    {#if activeTab === 'Home'}
      <header class="hero">
        <h1>Distributed Inference, Simplified</h1>
        <p>{summary}</p>
        <div class="actions">
          <button class="primary" on:click={() => run('run_cluster_start', { nodeCount: 2, basePort: 46000 })} disabled={busy}>{busy ? 'Starting...' : 'Start Cluster'}</button>
          <button on:click={() => run('run_flow_quick')} disabled={busy}>Run Flow</button>
          <button on:click={() => run('run_probe', { nodeId: 'studio-local', full: false })} disabled={busy}>Probe Host</button>
          <select bind:value={validationTier}>
            <option value="fast">Validation: Fast</option>
            <option value="full">Validation: Full</option>
          </select>
          <button on:click={runValidation} disabled={busy}>{busy ? 'Validating...' : 'Run Validation'}</button>
          <button on:click={loadSnapshot} disabled={busy}>Refresh Snapshot</button>
        </div>
      </header>

      <section class="metrics-grid" aria-busy={initializing}>
        {#if initializing}
          {#each [1, 2, 3, 4] as _}
            <article class="metric-card loading-card">
              <span>Loading</span>
              <strong>...</strong>
            </article>
          {/each}
        {:else}
          {#each cards as card}
            <article class="metric-card">
              <span>{card.label}</span>
              <strong>{card.value}</strong>
            </article>
          {/each}
        {/if}
      </section>
      {#if validationReport}
        <section class="validation-report">
          <h3>{validationReport.tier.toUpperCase()} Validation</h3>
          <p>{validationReport.summary}</p>
          <div class="validation-steps">
            {#each validationReport.steps as step}
              <article class="validation-step" class:ok={step.ok} class:fail={!step.ok}>
                <h4>{step.name}</h4>
                <p>{step.ok ? 'PASS' : 'FAIL'} · {step.durationMs} ms</p>
                {#if !step.ok && step.stderr}
                  <pre>{step.stderr}</pre>
                {/if}
              </article>
            {/each}
          </div>
        </section>
      {/if}

      {#if snapshotHistory.length > 0 || validationHistory.length > 0}
        <section class="history-grid">
          <article class="history-card">
            <h3>Snapshot Health Trend</h3>
            <p>Recent startup checks with pass ratio and warning count.</p>
            {#each snapshotHistory as item}
              <div class="history-row">
                <span class="stamp">{item.time}</span>
                <div class="bar-track">
                  <div class="bar-fill" style={`width: ${item.passPct}%`}></div>
                </div>
                <span class="metric">{item.passed}/{item.total}</span>
              </div>
            {/each}
          </article>

          <article class="history-card">
            <h3>Validation Run Trend</h3>
            <p>Recent validation outcomes and total runtime.</p>
            {#if validationHistory.length === 0}
              <p class="empty-note">No validation run history yet.</p>
            {:else}
              {#each validationHistory as item}
                <div class="validation-history-row">
                  <span class="stamp">{item.time}</span>
                  <span class:item-pass={item.ok} class:item-fail={!item.ok}>{item.tier} · {item.ok ? 'PASS' : 'FAIL'}</span>
                  <span class="metric">{item.durationMs} ms</span>
                </div>
              {/each}
            {/if}
          </article>
        </section>
      {/if}
    {:else if activeTab === 'Cluster'}
      <header class="hero">
        <h1>Cluster Operations</h1>
        <p>{clusterSummary} · {workerDiscoverySummary}</p>
        <div class="actions">
          <button class="primary" on:click={discoverWorkers} disabled={busy}>{busy ? 'Discovering...' : 'Discover Workers'}</button>
          <button class="primary" on:click={connectFlow} disabled={busy}>{busy ? 'Connecting...' : 'Connect Local -> Remote'}</button>
          <button class="primary" on:click={connectAllReachableWorkers} disabled={busy || workerDiscovery.length === 0}>{busy ? 'Batch Connecting...' : 'Connect Selected/Reachable'}</button>
          <button on:click={selectAllReachableWorkers} disabled={busy || workerDiscovery.length === 0}>Select Reachable</button>
          <button on:click={clearWorkerSelection} disabled={busy || selectedWorkerIds.length === 0}>Clear Selection</button>
          <button on:click={() => refreshCluster(false)} disabled={busy}>Refresh Cluster</button>
          <button on:click={() => (showAdvancedClusterButtons = !showAdvancedClusterButtons)} disabled={busy}>
            {showAdvancedClusterButtons ? 'Hide Advanced Buttons' : 'Show Advanced Buttons'}
          </button>
          {#if showAdvancedClusterButtons}
            <button on:click={() => run('run_cluster_start', { nodeCount: Number(startNodeCount), basePort: Number(startBasePort) })} disabled={busy}>Start Local Cluster</button>
            <button on:click={() => run('run_probe', { nodeId: localNodeId, full: true })} disabled={busy}>Full Probe (Local)</button>
            <button on:click={() => run('run_flow_quick')} disabled={busy}>Run Legacy Quick Flow</button>
            <button on:click={() => refreshCluster(true)} disabled={busy}>Deep Refresh</button>
          {/if}
        </div>
      </header>

      <section class="worker-kpis">
        <article class="metric-card">
          <span>Discovered</span>
          <strong>{workerDiscovery.length}</strong>
        </article>
        <article class="metric-card">
          <span>Reachable</span>
          <strong>{reachableWorkerCount}</strong>
        </article>
        <article class="metric-card">
          <span>Selected</span>
          <strong>{selectedWorkerCount}</strong>
        </article>
        <article class="metric-card">
          <span>Selected + Reachable</span>
          <strong>{selectedReachableWorkerCount}</strong>
        </article>
      </section>

      <section class="cluster-controls">
        <article class="cluster-card">
          <h3>Discovery Settings</h3>
          <p>Enter worker IDs to probe. Defaults from config are auto-included.</p>
          <div class="actions">
            <input bind:value={workerProbeHints} placeholder="studio-local, studio-remote, workstation-a" />
            <label class="checkbox"><input type="checkbox" bind:checked={workerProbeFull} /> Full probe</label>
            <input type="number" min="50" max="10000" step="50" bind:value={tcpProbeTimeoutMs} placeholder="tcp timeout ms" />
          </div>
        </article>

        <article class="cluster-card">
          <h3>Connection Settings</h3>
          <p>Run flow directly between selected workers.</p>
          <div class="actions">
            <input bind:value={localNodeId} placeholder="local node id" />
            <input bind:value={remoteNodeId} placeholder="remote node id" />
            <select bind:value={flowTransport}>
              <option value="tcp">tcp</option>
              <option value="inmem">inmem</option>
              <option value="ibverbs">ibverbs</option>
              <option value="ucx">ucx</option>
            </select>
            <input type="number" min="16" max="512" step="16" bind:value={flowExecutionTokens} placeholder="tokens" />
            <input type="number" min="1" max="16" step="1" bind:value={flowMicroBatch} placeholder="micro-batch" />
          </div>
          <div class="actions">
            <input type="number" min="1" max="12" step="1" bind:value={startNodeCount} placeholder="cluster node count" />
            <input type="number" min="1024" max="65535" step="1" bind:value={startBasePort} placeholder="cluster base port" />
          </div>
        </article>
      </section>

      {#if workerDiscovery.length > 0}
        <section class="worker-discovery-grid">
          {#each workerDiscovery as worker}
            <article class="cluster-card" class:healthy={worker.available} class:degraded={!worker.available}>
              <h3>{worker.id}</h3>
              <p>{worker.available ? 'reachable' : 'unreachable'} · {worker.acceleration} · probe {worker.probeMode ?? worker.probe_mode}</p>
              <p>Workers: {worker.workers}</p>
              <p>System RAM: {(worker.systemMemoryGb ?? worker.system_memory_gb ?? 0).toFixed(1)} GB</p>
              <p>GPU VRAM: {(worker.gpuVramGb ?? worker.gpu_vram_gb ?? 0).toFixed(1)} GB</p>
              <label class="checkbox">
                <input type="checkbox" checked={isWorkerSelected(worker.id)} on:change={(event) => toggleWorkerSelection(worker.id, event.currentTarget.checked)} />
                Include in batch connect
              </label>
              {#if worker.error}
                <p class="worker-error">{worker.error}</p>
              {/if}
              <div class="actions tcp-target-row">
                <input value={tcpTargetFor(worker.id).host} on:input={(event) => updateTcpTarget(worker.id, 'host', event.currentTarget.value)} placeholder="tcp host" />
                <input type="number" min="1" max="65535" value={tcpTargetFor(worker.id).port} on:input={(event) => updateTcpTarget(worker.id, 'port', event.currentTarget.value)} placeholder="tcp port" />
                <button on:click={() => testWorkerTcp(worker.id)} disabled={busy}>Quick TCP Test</button>
              </div>
              {#if workerTcpResults[worker.id]}
                <p class:tcp-pass={workerTcpResults[worker.id].reachable} class:tcp-fail={!workerTcpResults[worker.id].reachable}>
                  TCP {workerTcpResults[worker.id].reachable ? 'OK' : 'FAIL'} at {workerTcpResults[worker.id].testedAt} ·
                  {#if workerTcpResults[worker.id].reachable}
                    latency {workerTcpResults[worker.id].latencyMs ?? workerTcpResults[worker.id].latency_ms ?? 'n/a'} ms
                  {:else}
                    {workerTcpResults[worker.id].error ?? 'connection failed'}
                  {/if}
                </p>
              {/if}
              <div class="actions">
                <button on:click={() => (localNodeId = worker.id)} disabled={busy}>Set Local</button>
                <button on:click={() => (remoteNodeId = worker.id)} disabled={busy}>Set Remote</button>
                <button class="primary" on:click={() => connectToWorker(worker.id)} disabled={busy || !worker.available}>Connect</button>
              </div>
            </article>
          {/each}
        </section>
      {/if}

      {#if batchConnectResults.length > 0}
        <section class="batch-results">
          <h3>Batch Connect Results</h3>
          <div class="batch-results-grid">
            {#each batchConnectResults as item}
              <article class="batch-result" class:ok={item.ok} class:fail={!item.ok}>
                <p><strong>{item.workerId}</strong> · {item.ok ? 'OK' : 'FAIL'} · {item.durationMs} ms</p>
                {#if item.error}
                  <p>{item.error}</p>
                {/if}
              </article>
            {/each}
          </div>
        </section>
      {/if}

      <section class="cluster-grid">
        {#each clusterNodes as node}
          <article class="cluster-card" class:healthy={node.health === 'healthy'} class:degraded={node.health === 'degraded'}>
            <h3>{node.id}</h3>
            <p>{node.acceleration} · {node.health}</p>
            <p>Workers: {node.workers}</p>
            <p>System RAM: {(node.systemMemoryGb ?? node.system_memory_gb ?? 0).toFixed(1)} GB</p>
            <p>GPU VRAM: {(node.gpuVramGb ?? node.gpu_vram_gb ?? 0).toFixed(1)} GB</p>
          </article>
        {/each}
      </section>
    {:else if activeTab === 'Doctor'}
      <header class="hero">
        <h1>Diagnostics Center</h1>
        <p>Run preflight diagnostics and inspect remediation details.</p>
        <div class="actions">
          <button class="primary" on:click={() => runDoctorJson(false)} disabled={busy}>Doctor (Standard)</button>
          <button on:click={() => runDoctorJson(true)} disabled={busy}>Doctor (Strict)</button>
        </div>
      </header>
      {#if doctorSummary}
        <section class="doctor-grid">
          <article class="metric-card">
            <span>Pass</span>
            <strong>{doctorSummary.pass}</strong>
          </article>
          <article class="metric-card">
            <span>Warn</span>
            <strong>{doctorSummary.warn}</strong>
          </article>
          <article class="metric-card">
            <span>Fail</span>
            <strong>{doctorSummary.fail}</strong>
          </article>
        </section>
        <section class="doctor-checks">
          {#each doctorSummary.checks as check}
            <article class="doctor-check">
              <h3>[{check.status}] {check.area}/{check.name}</h3>
              <p>{check.detail}</p>
              {#if check.fix}
                <p class="fix">FIX: {check.fix}</p>
              {/if}
            </article>
          {/each}
        </section>
      {/if}
    {:else if activeTab === 'Models'}
      <header class="hero">
        <h1>Model Management</h1>
        <p>Discover, pull, activate, delete, and verify models with live backend + Ollama status.</p>
        <div class="actions">
          <button on:click={refreshConnectivity} disabled={busy}>Refresh Connectivity</button>
          <button on:click={loadBackendModels} disabled={busy || !backendReachable}>Refresh Installed Models</button>
          <button on:click={loadHfModels} disabled={busy || hfLoading}>{hfLoading ? 'Refreshing HF...' : 'Refresh HF Catalog'}</button>
          <button class="primary" on:click={verifyModel} disabled={busy}>Verify Hugging Face Repo</button>
        </div>
      </header>

      <section class="model-check model-health-grid">
        <article class="metric-card">
          <span>Backend</span>
          <strong>{backendReachable ? 'Online' : 'Offline'}</strong>
        </article>
        <article class="metric-card">
          <span>Ollama</span>
          <strong>{ollamaReachable ? 'Connected' : 'Unavailable'}</strong>
        </article>
        <article class="metric-card">
          <span>Current Backend Model</span>
          <strong>{currentBackendModel || 'unknown'}</strong>
        </article>
        <article class="metric-card">
          <span>Installed Models</span>
          <strong>{backendModels.length}</strong>
        </article>
        <article class="metric-card">
          <span>Loaded Models</span>
          <strong>{backendModelStatus.loadedModels.length}</strong>
        </article>
        <article class="metric-card">
          <span>Downloads In Progress</span>
          <strong>{Object.keys(backendModelStatus.downloadingModels ?? {}).length}</strong>
        </article>
        <article class="metric-card">
          <span>HF Candidates</span>
          <strong>{hfModels.length}</strong>
        </article>
        <article class="metric-card">
          <span>Last Model Action</span>
          <strong>{modelActionMessage || 'none'}</strong>
        </article>
      </section>

      <section class="model-layout">
        <article class="card-shell model-control-panel">
          <h3>Model Controls</h3>
          <label>Preset
            <select on:change={(e) => applyPreset(e.currentTarget.value)}>
              <option value="">Select preset</option>
              {#each modelPresets as preset, index}
                <option value={index}>{preset.name} ({preset.quant})</option>
              {/each}
            </select>
          </label>
          <label>Hugging Face Repo
            <input bind:value={modelRepo} placeholder="repo id (owner/model)" />
          </label>
          <label>Verification File
            <input bind:value={modelFile} placeholder="config.json" />
          </label>
          <label>Selected Backend/Ollama Model
            <select bind:value={selectedBackendModel}>
              <option value="">Select backend model</option>
              {#each backendModels as model}
                <option value={model.name}>{model.name}</option>
              {/each}
            </select>
          </label>
          <div class="actions model-actions">
            <button on:click={downloadSelectedModel} disabled={busy || !selectedBackendModel || !backendReachable || !ollamaReachable}>Download/Pull</button>
            <button on:click={loadSelectedModel} disabled={busy || !selectedBackendModel || !backendReachable || !ollamaReachable}>Set Active</button>
            <button class="danger" on:click={deleteSelectedModel} disabled={busy || !selectedBackendModel || !backendReachable || !ollamaReachable}>Delete</button>
          </div>
          {#if !ollamaReachable}
            <p class="model-warning">Ollama is unavailable at {ollamaUrl}. Connect Ollama to enable pull/load/delete actions.</p>
          {/if}
          {#if connectivityDetail}
            <p class="model-connection-detail">{connectivityDetail}</p>
          {/if}
        </article>

        <article class="card-shell model-catalog-panel">
          <h3>Available Models To Download</h3>
          <label>Filter Catalog
            <input bind:value={modelCatalogFilter} placeholder="Search model id or family" />
          </label>
          <div class="model-catalog-grid">
            {#each filteredModelCatalog() as entry}
              <article class="model-catalog-item">
                <p class="catalog-title">{entry.id}</p>
                <p class="catalog-meta">{entry.family} · {entry.sizeHint} · {entry.source}</p>
                <div class="actions compact">
                  <button on:click={() => chooseCatalogModel(entry.id)} disabled={busy}>Use</button>
                  <button class="primary" on:click={() => downloadCatalogModel(entry.id)} disabled={busy || !backendReachable || !ollamaReachable}>Pull</button>
                </div>
              </article>
            {/each}
          </div>
        </article>
      </section>

      <section class="card-shell hf-catalog">
        <div class="actions">
          <h3>Hugging Face Hub</h3>
          <input bind:value={hfQuery} placeholder="Search Hugging Face models" />
          <button on:click={loadHfModels} disabled={busy || hfLoading}>{hfLoading ? 'Loading...' : 'Search'}</button>
        </div>
        {#if hfError}
          <p class="model-warning">Hugging Face query failed: {hfError}</p>
        {:else if hfModels.length === 0}
          <p class="model-connection-detail">No Hugging Face models returned for this query.</p>
        {:else}
          <div class="hf-model-grid">
            {#each hfModels as model}
              <article class="hf-model-item">
                <p class="catalog-title">{model.id}</p>
                <p class="catalog-meta">task: {model.pipeline_tag} · downloads: {Number(model.downloads ?? 0).toLocaleString()} · likes: {Number(model.likes ?? 0).toLocaleString()}</p>
                <p class="catalog-meta">gated: {model.gated} · config: {model.has_config ? 'yes' : 'no'} · tokenizer: {model.has_tokenizer ? 'yes' : 'no'}</p>
                <p class="catalog-meta">updated: {model.last_modified || 'unknown'}</p>
                <div class="actions compact">
                  <button on:click={() => useHfModel(model.id)} disabled={busy}>Use Repo</button>
                  <button class="primary" on:click={() => verifyHfCandidate(model.id)} disabled={busy}>Verify Download</button>
                </div>
              </article>
            {/each}
          </div>
        {/if}
      </section>

      <section class="card-shell installed-models">
        <h3>Installed Models</h3>
        {#if backendModels.length === 0}
          <p class="model-warning">No installed models detected from backend inventory.</p>
        {:else}
          <div class="installed-models-grid">
            {#each backendModels as model}
              <article class="installed-model-row" class:is-active={model.name === currentBackendModel}>
                <div>
                  <p class="installed-model-title">{model.name}</p>
                  <p class="installed-model-meta">{model.type} · {model.quantization} · {Number(model.size_gb ?? 0).toFixed(1)} GB · {model.status}</p>
                </div>
                <div class="actions compact">
                  <button on:click={() => setActiveModel(model.name)} disabled={busy || !backendReachable || !ollamaReachable}>Set Active</button>
                  <button class="danger" on:click={() => deleteModel(model.name)} disabled={busy || !backendReachable || !ollamaReachable}>Delete</button>
                </div>
              </article>
            {/each}
          </div>
        {/if}
      </section>

      {#if modelCheck}
        <section class="model-check">
          <article class="metric-card">
            <span>Repository</span>
            <strong>{modelCheck.repo}</strong>
          </article>
          <article class="metric-card">
            <span>File</span>
            <strong>{modelCheck.file}</strong>
          </article>
          <article class="metric-card">
            <span>Status</span>
            <strong>{modelCheck.ok ? 'PASS' : 'FAIL'}</strong>
          </article>
        </section>
      {/if}
    {:else if activeTab === 'Chat'}
      <header class="hero">
        <h1>Chat / Inference</h1>
        <p>Run live flow-backed inference checks and review runtime metrics.</p>
      </header>
      <div class="chat-layout">
        <section class="chat-panel card-shell">
          <h3>Prompt Builder</h3>
          <label>Model
            <input bind:value={chatModel} placeholder="model name" />
          </label>
          <label>Prompt
            <textarea bind:value={chatPrompt} placeholder="Ask something..." spellcheck="false" />
          </label>
          <div class="chat-controls">
            <label>Temperature
              <input type="range" min="0" max="1" step="0.1" bind:value={chatTemperature} />
              <span>{chatTemperature}</span>
            </label>
            <label>Max Tokens
              <input type="number" min="32" max="2048" step="32" bind:value={chatMaxTokens} />
            </label>
            <label class="checkbox">
              <input type="checkbox" bind:checked={chatDistributed} /> Distributed backend
            </label>
            <button class="primary" on:click={runChat} disabled={busy || !backendReachable}>{busy ? 'Generating...' : 'Generate'}</button>
          </div>
        </section>

        <section class="chat-side-column">
          {#if chatResult}
            <section class="chat-response card-shell">
              <h3>{chatResult.model} ({chatResult.backend})</h3>
              <p class="chat-response-text" bind:this={chatResponseEl} on:scroll={handleChatResponseScroll}>{chatResult.response}</p>
            </section>
          {:else}
            <section class="chat-response card-shell placeholder-panel">
              <h3>Awaiting Response</h3>
              <p class="chat-response-text" bind:this={chatResponseEl} on:scroll={handleChatResponseScroll}>Run Generate to populate live inference output and trace context.</p>
            </section>
          {/if}
          {#if chatHistory.length > 0}
            <section class="chat-history card-shell">
              <h3>Recent Exchanges</h3>
              <div class="chat-history-scroll" bind:this={chatHistoryScrollEl} on:scroll={handleChatHistoryScroll}>
                {#each chatHistory as entry}
                  <article class="chat-history-item">
                    <p class="prompt">Q: {entry.prompt}</p>
                    <p class="answer">A: {entry.response}</p>
                    <p class="meta">{entry.model} · {entry.backend}</p>
                  </article>
                {/each}
              </div>
            </section>
          {/if}
        </section>
      </div>
    {:else if activeTab === 'Settings'}
      <header class="hero">
        <h1>Settings</h1>
        <p>Edit runtime config and tune Studio accessibility preferences.</p>
        <div class="actions">
          <button class="primary" on:click={saveConfig} disabled={busy || !configLoaded}>Save Config</button>
          <button on:click={loadConfig} disabled={busy}>Reload</button>
          <button on:click={resetPreferences} disabled={busy}>Reset UI Prefs</button>
        </div>
      </header>
      <section class="ui-prefs">
        <label>Theme
          <select bind:value={uiTheme}>
            <option value="neon">Neon Dusk</option>
            <option value="slate">Slate Grid</option>
          </select>
        </label>
        <label>Font Scale
          <input type="range" min="0.9" max="1.2" step="0.05" bind:value={fontScale} />
          <span>{fontScale.toFixed(2)}x</span>
        </label>
        <label class="checkbox"><input type="checkbox" bind:checked={reducedMotion} /> Reduced Motion</label>
        <label class="checkbox"><input type="checkbox" bind:checked={highContrast} /> High Contrast</label>
      </section>
      <section class="profile-portability">
        <h3>LLM Backend Overrides</h3>
        <p>Optional values passed to chat requests and persisted in Studio profile/preferences.</p>
        <div class="actions">
          <input bind:value={ollamaUrl} placeholder="ollama base url (e.g. http://127.0.0.1:11434)" />
          <input bind:value={ollamaModel} placeholder="ollama model (e.g. neural-chat)" />
        </div>
      </section>
      <section class="profile-portability">
        <h3>Profile Portability</h3>
        <p>Export or import a Studio profile bundle (UI preferences + model/chat defaults + TOML content).</p>
        <div class="actions">
          <input bind:value={profileName} placeholder="profile name" />
          <input bind:value={profilePath} placeholder="profile path" />
          <button on:click={exportProfile} disabled={busy}>Export Profile</button>
          <button on:click={importProfile} disabled={busy}>Import Profile</button>
        </div>
      </section>
      <section class="settings-editor">
        <p class="config-path">Target: {configPath || 'unresolved'}</p>
        <textarea bind:value={configContent} spellcheck="false" />
      </section>
    {:else}
      <header class="hero">
        <h1>{activeTab}</h1>
        <p>This area is under active integration with the Ghostlink runtime.</p>
      </header>
    {/if}
  </main>

  <aside class="details">
    <div class="details-header">
      <h2>Details</h2>
      <span class="state-chip" class:busy={busy}>{busy ? 'RUNNING' : 'READY'}</span>
    </div>
    <p>{status}</p>
    <p class="cmd">{command}</p>
    <pre>{output}</pre>
  </aside>
</div>

{#if showOnboarding}
  <div class="onboarding-backdrop">
    <section class="onboarding-modal">
      <h2>Welcome to Ghostlink Studio</h2>
      <p>Quick start path:</p>
      <ol>
        <li>Use Cluster tab and run Refresh Cluster.</li>
        <li>Use Models tab and verify a preset repo.</li>
        <li>Use Chat tab and run a live flow-backed response check.</li>
        <li>Use Doctor tab for preflight health checks.</li>
      </ol>
      <div class="actions">
        <button class="primary" on:click={closeOnboarding}>Start Using Studio</button>
      </div>
    </section>
  </div>
{/if}
