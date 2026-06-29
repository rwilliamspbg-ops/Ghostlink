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

  onMount(async () => {
    try {
      const studio = await invoke('studio_status');
      status = `${studio.app}: ${studio.status}`;
      output = `Repo root: ${studio.repo_root}`;
      await loadSnapshot();
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
          <button class="primary" on:click={() => run('run_doctor', { strict: false })} disabled={busy}>Doctor (Standard)</button>
          <button on:click={() => run('run_doctor', { strict: true })} disabled={busy}>Doctor (Strict)</button>
        </div>
      </header>
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
