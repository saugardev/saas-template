import { HeroMorph } from "./hero-morph";

const foundations = [
  {
    number: "01",
    title: "Identity that is already serious",
    description:
      "Password auth, generic social OIDC, and opaque browser sessions are wired into one clear flow.",
  },
  {
    number: "02",
    title: "Tenant boundaries by default",
    description:
      "Workspaces, projects, roles, and scoped API keys keep every customer and environment in the right lane.",
  },
  {
    number: "03",
    title: "Agent access you can explain",
    description:
      "OAuth 2.1, OIDC, protected-resource discovery, consent, and Streamable HTTP MCP ship together.",
  },
] as const;

const steps = [
  ["Create a workspace", "Give each customer a clean tenant boundary and a default project."],
  ["Connect your product", "Use the Rust API from the web app, a service, or your own client."],
  ["Authorize an agent", "Grant one project and only the scopes that the agent needs."],
] as const;

const questions = [
  ["What is included?", "A Next.js app and landing page, an Axum and Tokio API, PostgreSQL persistence, OAuth/OIDC, scoped API keys, and a protected MCP endpoint."],
  ["Can I use my own identity provider?", "Yes. The starter keeps the OIDC boundary generic so your provider and deployment choices stay yours."],
  ["Is this tied to one cloud?", "No. The application and infrastructure stay separate, so you can point the same product at the cloud setup you already operate."],
] as const;

export default function Home() {
  const appUrl = process.env.APP_URL ?? "http://localhost:3000";
  const docsUrl = process.env.DOCS_URL ?? "http://localhost:3003";

  return (
    <main id="top">
      <a className="skip-link" href="#content">Skip to content</a>

      <nav className="site-nav" aria-label="Primary navigation">
        <a className="wordmark" href="#top" aria-label="Agent SaaS home">
          <BrandMark />
          <span>Agent SaaS</span>
        </a>
        <div className="nav-links">
          <a href="#features">Features</a>
          <a href="#workflow">How it works</a>
          <a href="#security">Security</a>
        </div>
        <div className="nav-actions">
          <details className="mobile-menu">
            <summary aria-label="Open navigation"><span aria-hidden="true">☰</span></summary>
            <div>
              <a href="#features">Features</a>
              <a href="#workflow">How it works</a>
              <a href="#security">Security</a>
              <a href={docsUrl}>Documentation</a>
            </div>
          </details>
          <a className="button button-quiet nav-docs" href={docsUrl}>Docs</a>
          <a className="button button-dark" href={appUrl}>Open app <Arrow /></a>
        </div>
      </nav>

      <div id="content">
        <section className="hero section-wrap" aria-labelledby="hero-title">
          <div className="hero-copy">
            <p className="eyebrow"><span className="status-dot" />Multi-tenant SaaS foundation</p>
            <h1 id="hero-title">Ship your product.<br />Not the plumbing.</h1>
            <p className="hero-morph-line">Built for <HeroMorph /> from day one.</p>
            <p className="hero-description">
              A quiet, production-minded starting point for SaaS products that need secure users, clean tenant boundaries, and agent access.
            </p>
            <div className="hero-actions">
              <a className="button button-dark button-large" href={appUrl}>Start building <Arrow /></a>
              <a className="button button-light button-large" href={docsUrl}>Read the guide</a>
            </div>
            <p className="hero-note">MIT licensed · self-hosted · your cloud</p>
          </div>

          <ProductPreview />
        </section>

        <section className="stack-strip section-wrap" aria-label="Technology stack">
          <p>Built on boring, durable primitives</p>
          <ul>
            {['Next.js', 'Axum', 'Tokio', 'PostgreSQL', 'OAuth 2.1', 'MCP'].map((item) => <li key={item}>{item}</li>)}
          </ul>
        </section>

        <section className="content-section section-wrap" id="features" aria-labelledby="features-title">
          <header className="section-heading">
            <p className="eyebrow">The foundation</p>
            <h2 id="features-title">Everything around your idea,<br />already handled.</h2>
            <p>Keep the reliable parts predictable so your team can spend its attention on what customers actually buy.</p>
          </header>
          <div className="feature-grid">
            {foundations.map((feature) => (
              <article className="feature-card" key={feature.title}>
                <span className="feature-number">{feature.number}</span>
                <div className="feature-symbol" aria-hidden="true"><span /></div>
                <h3>{feature.title}</h3>
                <p>{feature.description}</p>
              </article>
            ))}
          </div>
        </section>

        <section className="workflow-section section-wrap" id="workflow" aria-labelledby="workflow-title">
          <div className="workflow-intro">
            <p className="eyebrow">A short path to useful</p>
            <h2 id="workflow-title">Three boundaries.<br />One clear flow.</h2>
            <p>Your users sign in, choose their context, and authorize tools without leaking product-specific decisions across the stack.</p>
          </div>
          <ol className="step-list">
            {steps.map(([title, description], index) => (
              <li key={title}>
                <span>0{index + 1}</span>
                <div><h3>{title}</h3><p>{description}</p></div>
              </li>
            ))}
          </ol>
        </section>

        <section className="security-section section-wrap" id="security" aria-labelledby="security-title">
          <div className="security-copy">
            <p className="eyebrow eyebrow-light">Security is the product</p>
            <h2 id="security-title">Sensible defaults,<br />visible boundaries.</h2>
            <p>Credentials are treated like credentials. Tenant context is explicit. Agent grants are narrow and inspectable.</p>
            <a className="button button-white" href={docsUrl}>Read the security model <Arrow /></a>
          </div>
          <div className="security-panel" aria-label="Security properties">
            <div><span>Session storage</span><strong>Opaque &amp; HTTP-only</strong></div>
            <div><span>API credentials</span><strong>Hashed at rest</strong></div>
            <div><span>Agent grants</span><strong>Project scoped</strong></div>
            <div><span>Authorization</span><strong>OAuth 2.1 + OIDC</strong></div>
            <div><span>MCP transport</span><strong>Streamable HTTP</strong></div>
          </div>
        </section>

        <section className="faq-section section-wrap" aria-labelledby="faq-title">
          <header className="section-heading compact">
            <p className="eyebrow">Questions, answered</p>
            <h2 id="faq-title">The useful details.</h2>
          </header>
          <div className="faq-list">
            {questions.map(([question, answer]) => (
              <details key={question}>
                <summary>{question}<span aria-hidden="true">+</span></summary>
                <p>{answer}</p>
              </details>
            ))}
          </div>
        </section>

        <section className="final-cta section-wrap" aria-labelledby="cta-title">
          <BrandMark />
          <p className="eyebrow">Start with the foundation</p>
          <h2 id="cta-title">Build the part only you can build.</h2>
          <p>Identity, tenancy, and agent access are ready when you are.</p>
          <div>
            <a className="button button-dark button-large" href={appUrl}>Open the app <Arrow /></a>
            <a className="button button-light button-large" href={docsUrl}>Explore the docs</a>
          </div>
        </section>
      </div>

      <footer className="site-footer section-wrap">
        <a className="wordmark" href="#top" aria-label="Agent SaaS home"><BrandMark /><span>Agent SaaS</span></a>
        <p>Next.js · Rust · PostgreSQL</p>
        <a href={docsUrl}>Documentation <Arrow /></a>
      </footer>
    </main>
  );
}

function BrandMark() {
  return <span className="brand-mark" aria-hidden="true"><span>✦</span></span>;
}

function Arrow() {
  return <span className="arrow" aria-hidden="true">↗</span>;
}

function ProductPreview() {
  const rows = [
    ["Atlas", "Production", "Active"],
    ["Beacon", "Staging", "Active"],
    ["Courier", "Development", "Draft"],
    ["Delta", "Production", "Active"],
    ["Echo", "Preview", "Draft"],
    ["Foundry", "Production", "Active"],
    ["Gateway", "Staging", "Active"],
    ["Harbor", "Development", "Draft"],
  ];

  return (
    <figure className="product-preview">
      <div className="browser-bar" aria-hidden="true">
        <span className="browser-dots"><i /><i /><i /></span>
        <span className="browser-control">‹</span><span className="browser-control faded">›</span>
        <span className="browser-address"><i /> app.local/dashboard</span>
      </div>
      <div className="preview-brand" aria-hidden="true"><BrandMark /><span>Agent SaaS</span><small>Secure</small></div>
      <div className="preview-layout" aria-hidden="true">
        <aside className="preview-sidebar">
          <p>Product</p>
          <div className="preview-nav active"><span className="grid-icon" />Projects<i /></div>
          <div className="preview-nav"><span className="users-icon" />Workspaces</div>
          <div className="preview-nav"><span className="key-icon" />API keys</div>
          <p>Resources</p>
          <div className="preview-nav"><span className="doc-icon" />MCP guide</div>
          <div className="preview-nav"><span className="doc-icon" />Documentation</div>
          <p>Recents</p>
          <div className="preview-recent active">Production<i /></div>
          <div className="preview-recent">Staging</div>
          <div className="preview-recent">Preview</div>
          <div className="preview-help"><strong>Connect an agent ↗</strong><span>Authorize tools with a project-scoped grant.</span></div>
          <div className="preview-profile"><b>SD</b><span><strong>Sam Dev</strong><small>Starter workspace</small></span></div>
        </aside>
        <div className="preview-main">
          <header><div><span>Production / Projects</span><h3>Projects</h3></div><span className="preview-button">＋ New project</span></header>
          <div className="preview-filters"><span>All workspaces⌄</span><span>All states⌄</span></div>
          <div className="preview-chart"><div><span>Active projects</span><strong>12</strong></div><div className="chart-bars">{Array.from({ length: 28 }, (_, index) => <i className={index === 7 || index === 19 || index > 24 ? 'hot' : ''} key={index} />)}</div></div>
          <div className="preview-table">
            <div className="preview-row heading"><span>Project</span><span>Environment</span><span>Status</span></div>
            {rows.map(([project, environment, state], index) => (
              <div className="preview-row" key={project}><span><i className={index === 1 ? 'checked' : ''} />{project}</span><span>{environment}</span><span><b className={state === 'Draft' ? 'draft' : ''} />{state}</span></div>
            ))}
          </div>
        </div>
      </div>
      <figcaption className="sr-only">A preview of the project dashboard with navigation, project activity, filters, and a project table.</figcaption>
    </figure>
  );
}
