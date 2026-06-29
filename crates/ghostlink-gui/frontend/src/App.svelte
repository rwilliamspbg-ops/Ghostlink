<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  const navItems = ['Home', 'Models', 'Chat', 'Cluster', 'Settings', 'Doctor'];
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
  let modelPresets = [];
  let modelCheck = null;
  let chatPrompt = '';
  let chatModel = 'ghostlink-live-7b';
  let chatTemperature = 0.7;
  let chatMaxTokens = 256;
  let chatDistributed = true;
  let chatResult = null;
  let chatHistory = [];
  let clusterNodes = [];
  let clusterSummary = 'No live cluster snapshot loaded.';
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
      chatHistory,
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
      chatHistory = Array.isArray(prefs.chatHistory) ? prefs.chatHistory.slice(0, 12) : [];
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
    applyVisualPreferences();
    persistPreferences();
  }

  async function exportProfile() {
    busy = true;
    try {
      const result = await invoke('export_studio_profile', {
        profileName,
        uiTheme,
        fontScale: Number(fontScale),
        reducedMotion,
        highContrast,
        modelRepo,
        modelFile,
        chatModel,
        chatDistributed,
        configContent,
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
      const profile = await invoke('import_studio_profile', { profilePath });
      profileName = profile.profileName;
      uiTheme = profile.uiTheme;
      fontScale = Number(profile.fontScale);
      reducedMotion = Boolean(profile.reducedMotion);
      highContrast = Boolean(profile.highContrast);
      modelRepo = profile.modelRepo;
      modelFile = profile.modelFile;
      chatModel = profile.chatModel;
      chatDistributed = Boolean(profile.chatDistributed);
      configContent = profile.configContent;
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
    const snapshot = await invoke('studio_snapshot');
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
      const result = await invoke(action, args);
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
      const cfg = await invoke('load_ghostlink_config');
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
      const cfg = await invoke('save_ghostlink_config', { content: configContent });
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
      const report = await invoke('run_doctor_with_json', { strict });
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
      const result = await invoke('verify_hf_repo', {
        repo: modelRepo,
        file: modelFile,
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
    const presets = await invoke('list_model_presets');
    modelPresets = presets;
  }

  function applyPreset(indexValue) {
    const index = Number(indexValue);
    if (!Number.isInteger(index) || index < 0 || index >= modelPresets.length) {
      return;
    }

    const preset = modelPresets[index];
    modelRepo = preset.repo;
    modelFile = preset.defaultFile;
  }

  async function refreshCluster(full = false) {
    busy = true;
    try {
      const snapshot = await invoke('cluster_preview', { nodeId: 'studio-local', full });
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
      const result = await invoke('chat_infer', {
        prompt: chatPrompt,
        model: chatModel,
        temperature: Number(chatTemperature),
        maxTokens: Number(chatMaxTokens),
        distributed: chatDistributed,
      });
      chatResult = result;
      status = `Chat response generated via ${result.backend}`;
      command = 'chat_infer';
      output = result.trace;
      chatHistory = [
        {
          prompt: chatPrompt,
          response: result.response,
          model: result.model,
          backend: result.backend,
        },
        ...chatHistory,
      ].slice(0, 12);
      persistPreferences();
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
      const report = await invoke('run_validation_tier', { tier: validationTier });
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
      const studio = await invoke('studio_status');
      status = `${studio.app}: ${studio.status}`;
      output = `Repo root: ${studio.repo_root}`;
      await loadSnapshot();
      await loadConfig();
      await loadModelPresets();
      await refreshCluster(false);
    } catch (err) {
      status = 'Studio bridge unavailable';
      output = String(err);
    }
  });

  $: applyVisualPreferences();
  $: persistPreferences();
</script>

<div class="studio-shell">
  <aside class="sidebar">
    <div class="brand">Ghostlink Studio</div>
    {#each navItems as item}
      <button class="nav-item" class:active={item === activeTab} on:click={() => (activeTab = item)}>{item}</button>
    {/each}
  </aside>

  <main class="dashboard">
    {#if activeTab === 'Home'}
      <header class="hero">
        <h1>Distributed Inference, Simplified</h1>
        <p>{summary}</p>
        <div class="actions">
          <button class="primary" on:click={() => run('run_cluster_start', { nodeCount: 2, basePort: 46000 })} disabled={busy}>Start Cluster</button>
          <button on:click={() => run('run_flow_quick')} disabled={busy}>Run Flow</button>
          <button on:click={() => run('run_probe', { nodeId: 'studio-local', full: false })} disabled={busy}>Probe Host</button>
          <select bind:value={validationTier}>
            <option value="fast">Validation: Fast</option>
            <option value="full">Validation: Full</option>
          </select>
          <button on:click={runValidation} disabled={busy}>Run Validation</button>
          <button on:click={loadSnapshot} disabled={busy}>Refresh Snapshot</button>
        </div>
      </header>

      <section class="metrics-grid">
        {#each cards as card}
          <article class="metric-card">
            <span>{card.label}</span>
            <strong>{card.value}</strong>
          </article>
        {/each}
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
        <p>{clusterSummary}</p>
        <div class="actions">
          <button class="primary" on:click={() => run('run_cluster_start', { nodeCount: 3, basePort: 46000 })} disabled={busy}>Start 3-Node Local Cluster</button>
          <button on:click={() => run('run_probe', { nodeId: 'studio-local', full: true })} disabled={busy}>Full Probe</button>
          <button on:click={() => run('run_flow_quick')} disabled={busy}>Run Quick Flow</button>
          <button on:click={() => refreshCluster(false)} disabled={busy}>Refresh Cluster</button>
          <button on:click={() => refreshCluster(true)} disabled={busy}>Deep Refresh</button>
        </div>
      </header>
      <section class="cluster-grid">
        {#each clusterNodes as node}
          <article class="cluster-card" class:healthy={node.health === 'healthy'} class:degraded={node.health === 'degraded'}>
            <h3>{node.id}</h3>
            <p>{node.acceleration} · {node.health}</p>
            <p>Workers: {node.workers}</p>
            <p>System RAM: {node.systemMemoryGb.toFixed(1)} GB</p>
            <p>GPU VRAM: {node.gpuVramGb.toFixed(1)} GB</p>
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
        <p>Verify Hugging Face model accessibility and basic repository readiness.</p>
        <div class="actions">
          <select on:change={(e) => applyPreset(e.currentTarget.value)}>
            <option value="">Select preset</option>
            {#each modelPresets as preset, index}
              <option value={index}>{preset.name} ({preset.quant})</option>
            {/each}
          </select>
          <input bind:value={modelRepo} placeholder="repo id (owner/model)" />
          <input bind:value={modelFile} placeholder="file" />
          <button class="primary" on:click={verifyModel} disabled={busy}>Verify Model</button>
        </div>
      </header>
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
      <section class="chat-panel">
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
          <button class="primary" on:click={runChat} disabled={busy}>Generate</button>
        </div>
      </section>
      {#if chatResult}
        <section class="chat-response">
          <h3>{chatResult.model} ({chatResult.backend})</h3>
          <p>{chatResult.response}</p>
        </section>
      {/if}
      {#if chatHistory.length > 0}
        <section class="chat-history">
          <h3>Recent Exchanges</h3>
          {#each chatHistory as entry}
            <article class="chat-history-item">
              <p class="prompt">Q: {entry.prompt}</p>
              <p class="answer">A: {entry.response}</p>
              <p class="meta">{entry.model} · {entry.backend}</p>
            </article>
          {/each}
        </section>
      {/if}
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
    <h2>Details</h2>
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
