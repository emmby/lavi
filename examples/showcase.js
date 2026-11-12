/**
 * Shared interactive statechart shell for Lavi WASM showcase demos.
 * Handles xstate-diagram SVG loading, active state & guard highlighting,
 * click-to-send transitions, Diagram/JSON view toggle, Zoom, and Maximize.
 */

/**
 * Flattens a primary branch of a snapshot value (e.g. `{ moving: { cleaning: 'patrol' } }`)
 * into an array of state keys (`['moving', 'cleaning', 'patrol']`).
 */
export function extractStateParts(val) {
    if (typeof val === 'string') return [val];
    if (val && typeof val === 'object') {
        const keys = Object.keys(val);
        if (keys.length > 0) {
            return [keys[0], ...extractStateParts(val[keys[0]])];
        }
    }
    return [];
}

/**
 * Collects every active dot-delimited state path from a snapshot value
 * (supports atomic, compound, and parallel state values).
 */
export function getActivePaths(val, prefix = '', out = new Set()) {
    if (typeof val === 'string') {
        out.add(prefix ? `${prefix}.${val}` : val);
        return out;
    }
    if (val && typeof val === 'object') {
        for (const [key, child] of Object.entries(val)) {
            const next = prefix ? `${prefix}.${key}` : key;
            out.add(next);
            getActivePaths(child, next, out);
        }
    }
    return out;
}

/**
 * Initializes the shared Statechart card (Diagram SVG, Machine JSON, Zoom, Maximize,
 * click-to-send transitions, and live snapshot syncing).
 */
export async function initStatechartPanel({
    actor,
    machineDef,
    svgUrl,
    svgLabel = 'Interactive Statechart',
    stateBadgeEl = document.getElementById('lblState'),
    stateNotes = null,
    explanationEl = document.getElementById('lblExplanation'),
    onBeforeSend = null,
    onAfterSync = null
}) {
    const statechartCard = document.getElementById('statechartCard');
    const diagramView = document.getElementById('diagramView');
    const jsonViewEl = document.getElementById('jsonView');
    const btnShowDiagram = document.getElementById('btnShowDiagram');
    const btnShowJson = document.getElementById('btnShowJson');
    const btnToggleZoom = document.getElementById('btnToggleZoom');
    const btnToggleMaximize = document.getElementById('btnToggleMaximize');

    if (jsonViewEl) {
        jsonViewEl.textContent =
            '// Machine Definition (passed to lavi-wasm)\n' +
            JSON.stringify(machineDef, null, 2);
    }

    let statechartSvg = null;
    if (svgUrl && diagramView) {
        const res = await fetch(svgUrl);
        if (!res.ok) {
            throw new Error(`Failed to load ${svgUrl} (${res.status})`);
        }
        diagramView.innerHTML = await res.text();
        statechartSvg = diagramView.querySelector('svg');
        if (statechartSvg) {
            statechartSvg.id = 'statechartSvg';
            statechartSvg.setAttribute('aria-label', svgLabel);
        }
    }

    if (btnToggleZoom && diagramView) {
        btnToggleZoom.addEventListener('click', () => {
            const zoomed = diagramView.classList.toggle('zoomed');
            btnToggleZoom.classList.toggle('active', zoomed);
            btnToggleZoom.setAttribute('aria-pressed', String(zoomed));
            btnToggleZoom.textContent = zoomed ? 'Fit View' : 'Zoom 100%';
            if (diagramView.style.display !== 'none') {
                diagramView.style.display = zoomed ? 'block' : 'flex';
            }
        });
    }

    function setMaximized(maximized) {
        if (!statechartCard || !btnToggleMaximize) return;
        statechartCard.classList.toggle('maximized', maximized);
        btnToggleMaximize.classList.toggle('active', maximized);
        btnToggleMaximize.setAttribute('aria-pressed', String(maximized));
        btnToggleMaximize.textContent = maximized ? 'Exit Fullscreen' : 'Maximize';
    }

    if (btnToggleMaximize) {
        btnToggleMaximize.addEventListener('click', () => {
            setMaximized(!statechartCard.classList.contains('maximized'));
        });
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && statechartCard?.classList.contains('maximized')) {
                setMaximized(false);
            }
        });
    }

    if (btnShowDiagram && btnShowJson && diagramView && jsonViewEl) {
        btnShowDiagram.addEventListener('click', () => {
            btnShowDiagram.classList.add('active');
            btnShowDiagram.setAttribute('aria-selected', 'true');
            btnShowJson.classList.remove('active');
            btnShowJson.setAttribute('aria-selected', 'false');
            diagramView.style.display = diagramView.classList.contains('zoomed') ? 'block' : 'flex';
            if (btnToggleZoom) btnToggleZoom.style.display = '';
            jsonViewEl.style.display = 'none';
        });

        btnShowJson.addEventListener('click', () => {
            btnShowJson.classList.add('active');
            btnShowJson.setAttribute('aria-selected', 'true');
            btnShowDiagram.classList.remove('active');
            btnShowDiagram.setAttribute('aria-selected', 'false');
            diagramView.style.display = 'none';
            if (btnToggleZoom) btnToggleZoom.style.display = 'none';
            jsonViewEl.style.display = 'block';
        });
    }

    function flashTriggeredEdge(eventName, sourcePaths) {
        if (!statechartSvg) return;
        statechartSvg.querySelectorAll('g.transition-edge[data-edge-event]').forEach((edge) => {
            if (
                edge.getAttribute('data-edge-event') === eventName &&
                sourcePaths.has(edge.getAttribute('data-edge-source-path'))
            ) {
                edge.classList.remove('is-triggered');
                void edge.offsetWidth;
                edge.classList.add('is-triggered');
                setTimeout(() => edge.classList.remove('is-triggered'), 550);
            }
        });
    }

    function triggerEvent(eventName, payload) {
        const snap = actor.getSnapshot();
        const eventObj = payload ? { type: eventName, ...payload } : { type: eventName };
        if (!snap.can(eventObj)) return false;

        const activePaths = getActivePaths(snap.value);
        if (onBeforeSend) {
            onBeforeSend(eventName, snap);
        }

        flashTriggeredEdge(eventName, activePaths);
        actor.send(eventObj);
        syncStatechart();
        return true;
    }

    if (statechartSvg) {
        statechartSvg.addEventListener('click', (e) => {
            const edge = e.target.closest('g.transition-edge.is-enabled[data-edge-event]');
            if (edge) {
                triggerEvent(edge.getAttribute('data-edge-event'));
                return;
            }
            const internalRow = e.target.closest('.internal-transition.is-enabled[data-internal-event]');
            if (internalRow) {
                triggerEvent(internalRow.getAttribute('data-internal-event'));
            }
        });
    }

    function syncStatechart() {
        const snap = actor.getSnapshot();
        const parts = extractStateParts(snap.value);
        const dotPath = parts.join('.');
        const leaf = parts[parts.length - 1] || '';
        const activePaths = getActivePaths(snap.value);

        if (stateBadgeEl) {
            stateBadgeEl.textContent = dotPath;
        }

        if (explanationEl && stateNotes && stateNotes[leaf]) {
            explanationEl.innerHTML = `<strong>What's happening:</strong> ${stateNotes[leaf]}`;
        }

        if (statechartSvg) {
            statechartSvg.querySelectorAll('g.state-node[data-state-path]').forEach((node) => {
                const path = node.getAttribute('data-state-path');
                node.classList.toggle('is-active', activePaths.has(path));
            });

            statechartSvg.querySelectorAll('g.transition-edge[data-edge-source-path]').forEach((edge) => {
                const sourcePath = edge.getAttribute('data-edge-source-path');
                const eventName = edge.getAttribute('data-edge-event');
                const isActiveSource = activePaths.has(sourcePath);
                const isTimed = !eventName || eventName.startsWith('after(');
                const canFire = isActiveSource && !isTimed && snap.can({ type: eventName });
                edge.classList.toggle('is-enabled', Boolean(canFire));
                edge.classList.toggle('is-disabled', Boolean(isActiveSource && !isTimed && !canFire));
            });

            statechartSvg.querySelectorAll('.internal-transition[data-internal-event]').forEach((row) => {
                const stateNode = row.closest('g.state-node[data-state-path]');
                const statePath = stateNode ? stateNode.getAttribute('data-state-path') : '';
                const eventName = row.getAttribute('data-internal-event');
                const isActiveSource = activePaths.has(statePath);
                const canFire = isActiveSource && eventName && snap.can({ type: eventName });
                row.classList.toggle('is-enabled', Boolean(canFire));
            });
        }

        if (onAfterSync) {
            onAfterSync(snap, { parts, dotPath, leaf, activePaths });
        }
    }

    return {
        syncStatechart,
        triggerEvent,
        flashTriggeredEdge,
        getSvg: () => statechartSvg
    };
}
