/**
 * Seed content for Phase 2.
 *
 * State lives in memory only — there is no persistence until Phase 3, so the
 * editor opens on this every time. It doubles as a working sample of every
 * block type the phase is meant to support.
 */
export const INITIAL_CONTENT = `
<h1>Sb₂Se₃ growth log</h1>
<p>Antimony selenide by <a class="sutra-link" href="#">closed-space vapour transport</a>. Quasi-1D ribbons along [001], which is the whole reason the deposition angle matters.</p>

<h2>Run conditions</h2>
<table>
  <tr><th>Parameter</th><th>Value</th><th>Note</th></tr>
  <tr><td>Source</td><td>560 °C</td><td>Zone 1</td></tr>
  <tr><td>Substrate</td><td>380 °C</td><td>Zone 2, FTO glass</td></tr>
  <tr><td>Duration</td><td>25 min</td><td>Shutter opened at t = 3 min</td></tr>
</table>

<h2>Observations</h2>
<ul>
  <li>Ribbon texture visible by eye at the substrate edge</li>
  <li>Coverage falls off past ~12 mm from centre
    <ul><li>Likely a shadowing artefact from the mask</li></ul>
  </li>
</ul>

<blockquote><p>Grain orientation tracks the substrate temperature more tightly than the source temperature. Worth isolating.</p></blockquote>

<h3>Next</h3>
<ul data-type="taskList">
  <li data-type="taskItem" data-checked="true"><label><input type="checkbox" checked><span></span></label><div><p>XRD on the 380 °C run</p></div></li>
  <li data-type="taskItem" data-checked="false"><label><input type="checkbox"><span></span></label><div><p>Repeat at 400 °C substrate</p></div></li>
  <li data-type="taskItem" data-checked="false"><label><input type="checkbox"><span></span></label><div><p>Cross-section SEM, measure ribbon tilt</p></div></li>
</ul>

<hr>

<p>Peak assignment, from the indexing script:</p>
<pre><code>hkl   2theta   I/I0
120   28.2     100
221   31.1      64
211   17.6      31</code></pre>

<ol>
  <li>Normalise to the 120 reflection</li>
  <li>Compare against ICSD 2118</li>
  <li>Flag anything above 5% deviation</li>
</ol>

<p>Type <code>/</code> on a new line to insert a block. Hover any block for its drag handle.</p>
`;
