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
  let configPath = '';
  let configContent = '';
  let configLoaded = false;
  let doctorSummary = null;
  let busy = false;

  async function loadSnapshot() {
    const snapshot = await invoke('studio_snapshot');
    cards = snapshot.metrics.map((metric) => ({ label: metric.label, value: metric.value }));
    summary = snapshot.summary;
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

  onMount(async () => {
    try {
      const studio = await invoke('studio_status');
      status = `${studio.app}: ${studio.status}`;
      output = `Repo root: ${studio.repo_root}`;
      await loadSnapshot();
      await loadConfig();
    } catch (err) {
      status = 'Studio bridge unavailable';
      output = String(err);
    }
  });
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
    {:else if activeTab === 'Cluster'}
      <header class="hero">
        <h1>Cluster Operations</h1>
        <p>Launch local listeners, probe host state, and run a quick distributed flow.</p>
        <div class="actions">
          <button class="primary" on:click={() => run('run_cluster_start', { nodeCount: 3, basePort: 46000 })} disabled={busy}>Start 3-Node Local Cluster</button>
          <button on:click={() => run('run_probe', { nodeId: 'studio-local', full: true })} disabled={busy}>Full Probe</button>
          <button on:click={() => run('run_flow_quick')} disabled={busy}>Run Quick Flow</button>
        </div>
      </header>
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
    {:else if activeTab === 'Settings'}
      <header class="hero">
        <h1>Settings</h1>
        <p>Edit and save Ghostlink TOML configuration directly from Studio.</p>
        <div class="actions">
          <button class="primary" on:click={saveConfig} disabled={busy || !configLoaded}>Save Config</button>
          <button on:click={loadConfig} disabled={busy}>Reload</button>
        </div>
      </header>
      <section class="settings-editor">
        <p class="config-path">Target: {configPath || 'unresolved'}</p>
        <textarea bind:value={configContent} spellcheck="false" />
      </section>
    {:else}
      <header class="hero">
        <h1>{activeTab}</h1>
        <p>This area is scaffolded for Sprint 2 integration.</p>
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
