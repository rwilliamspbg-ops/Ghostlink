<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  const navItems = ['Home', 'Models', 'Chat', 'Cluster', 'Settings', 'Doctor'];
  let cards = [
    { label: 'Nodes Online', value: '0' },
    { label: 'Total VRAM', value: '0 GB' },
    { label: 'Throughput', value: '0 tok/s' },
    { label: 'Avg Latency', value: '0 ms' },
  ];

  let status = 'Loading...';
  let command = '';
  let output = 'No command executed yet.';
  let busy = false;

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
        cards = [
          { label: 'Nodes Online', value: '1' },
          { label: 'Total VRAM', value: 'Detected' },
          { label: 'Throughput', value: 'Measured' },
          { label: 'Avg Latency', value: 'Measured' },
        ];
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
      <button class="nav-item">{item}</button>
    {/each}
  </aside>

  <main class="dashboard">
    <header class="hero">
      <h1>Distributed Inference, Simplified</h1>
      <p>Start your cluster and run diagnostics in one click.</p>
      <div class="actions">
        <button class="primary" on:click={() => run('run_cluster_start', { nodeCount: 2, basePort: 46000 })} disabled={busy}>Start Cluster</button>
        <button on:click={() => run('run_flow_quick')} disabled={busy}>Run Flow</button>
        <button on:click={() => run('run_doctor', { strict: false })} disabled={busy}>Open Doctor</button>
        <button on:click={() => run('run_probe', { nodeId: 'studio-local', full: false })} disabled={busy}>Probe Host</button>
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
  </main>

  <aside class="details">
    <h2>Details</h2>
    <p>{status}</p>
    <p class="cmd">{command}</p>
    <pre>{output}</pre>
  </aside>
</div>
