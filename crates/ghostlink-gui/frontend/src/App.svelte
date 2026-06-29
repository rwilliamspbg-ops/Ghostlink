<script>
  const navItems = ['Home', 'Models', 'Chat', 'Cluster', 'Settings', 'Doctor'];
  const cards = [
    { label: 'Nodes Online', value: '0' },
    { label: 'Total VRAM', value: '0 GB' },
    { label: 'Throughput', value: '0 tok/s' },
    { label: 'Avg Latency', value: '0 ms' },
  ];
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
        <button class="primary">Start Cluster</button>
        <button>Run Flow</button>
        <button>Open Doctor</button>
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
    <p>Cluster details, health signals, and remediation hints will appear here.</p>
  </aside>
</div>
