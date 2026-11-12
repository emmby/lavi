
const { test, expect } = require('@playwright/test');

test.describe('FSM WASM Examples', () => {

  test.beforeEach(async ({ page }) => {
    page.on('console', msg => {
      if (msg.type() === 'error') {
        throw new Error(`Console Error: "${msg.text()}"`);
      }
    });
    // Also fail on uncaught exceptions
    page.on('pageerror', err => {
      throw new Error(`Page Error: "${err.message}"`);
    });
  });

  test('Smart Dimmer', async ({ page }) => {
    await page.goto('/examples/smart_dimmer.html');

    const bulb = page.locator('#bulb');
    const stateLabel = page.locator('#lblState');
    const toggleBtn = page.locator('#btnToggle');
    const dimBtn = page.locator('#btnDim');
    const brightenBtn = page.locator('#btnBrighten');
    const brightnessLabel = page.locator('#lblBrightness');
    const togglesLabel = page.locator('#lblToggles');

    // Initial State: off, brightness 80%
    await expect(bulb).not.toHaveClass(/on/);
    await expect(stateLabel).toHaveText('off');
    await expect(brightnessLabel).toHaveText('80%');
    await expect(dimBtn).toBeDisabled();
    await expect(brightenBtn).toBeDisabled();

    // Toggle -> on
    await toggleBtn.click();
    await expect(bulb).toHaveClass(/on/);
    await expect(stateLabel).toHaveText('on');
    await expect(togglesLabel).toHaveText('1');
    await expect(dimBtn).toBeEnabled();
    await expect(brightenBtn).toBeEnabled();

    // Brighten -> 100% (brightenBtn now disabled by guard)
    await brightenBtn.click();
    await expect(brightnessLabel).toHaveText('100%');
    await expect(brightenBtn).toBeDisabled();

    // Dim -> 80%
    await dimBtn.click();
    await expect(brightnessLabel).toHaveText('80%');
    await expect(brightenBtn).toBeEnabled();

    // Toggle -> off (remembers 80%)
    await toggleBtn.click();
    await expect(bulb).not.toHaveClass(/on/);
    await expect(stateLabel).toHaveText('off');
    await expect(brightnessLabel).toHaveText('80%');
  });

  test('Traffic Light', async ({ page }) => {
    await page.goto('/examples/traffic_light.html');

    const red = page.locator('#red');
    const yellow = page.locator('#yellow');
    const green = page.locator('#green');
    const timerBtn = page.locator('#btnTimer');
    const pedestrianBtn = page.locator('#btnPedestrian');
    const sensorCheckbox = page.locator('#chkCrosswalkEnabled');
    const stateLabel = page.locator('#lblState');
    const cycleCount = page.locator('#lblCycleCount');
    const walkStatus = page.locator('#lblWalkStatus');

    const greenNode = page.locator('#statechartSvg g.state-node[data-state-path="green"]');
    const yellowNode = page.locator('#statechartSvg g.state-node[data-state-path="yellow"]');
    const redNode = page.locator('#statechartSvg g.state-node[data-state-path="red"]');

    // Pause auto clock so manual transitions are deterministic
    await page.locator('#btnToggleClock').click();

    // Initial State: green
    await expect(green).toHaveClass(/active/);
    await expect(yellow).not.toHaveClass(/active/);
    await expect(red).not.toHaveClass(/active/);
    await expect(stateLabel).toHaveText('green');
    await expect(cycleCount).toHaveText('0');
    await expect(walkStatus).toHaveText('WAIT');
    await expect(greenNode).toHaveClass(/is-active/);
    await expect(pedestrianBtn).toBeEnabled();

    // Timer -> yellow
    await timerBtn.click();
    await expect(green).not.toHaveClass(/active/);
    await expect(yellow).toHaveClass(/active/);
    await expect(stateLabel).toHaveText('yellow');
    await expect(yellowNode).toHaveClass(/is-active/);
    await expect(pedestrianBtn).toBeDisabled();

    // Timer -> red (entry increments cycleCount & sets WALK)
    await timerBtn.click();
    await expect(yellow).not.toHaveClass(/active/);
    await expect(red).toHaveClass(/active/);
    await expect(stateLabel).toHaveText('red');
    await expect(cycleCount).toHaveText('1');
    await expect(walkStatus).toHaveText('WALK');
    await expect(redNode).toHaveClass(/is-active/);

    // Timer -> green (exit clears WALK)
    await timerBtn.click();
    await expect(red).not.toHaveClass(/active/);
    await expect(green).toHaveClass(/active/);
    await expect(stateLabel).toHaveText('green');
    await expect(walkStatus).toHaveText('WAIT');

    // Guard check: disabling crosswalk sensor disables REQUEST_CROSSWALK
    await sensorCheckbox.uncheck();
    await expect(pedestrianBtn).toBeDisabled();
    await sensorCheckbox.check();
    await expect(pedestrianBtn).toBeEnabled();

    // Request Crosswalk -> yellow
    await pedestrianBtn.click();
    await expect(stateLabel).toHaveText('yellow');

    // Diagram / Machine JSON toggle
    await page.locator('#btnShowJson').click();
    await expect(page.locator('#jsonView')).toBeVisible();
    await expect(page.locator('#jsonView')).toContainText('"id": "trafficLight"');
    await page.locator('#btnShowDiagram').click();
    await expect(page.locator('#diagramView')).toBeVisible();
  });

  test('Vending Machine', async ({ page }) => {
    await page.goto('/examples/vending_machine.html');

    const stateLabel = page.locator('#lblState');
    const balanceLabel = page.locator('#lblBalance');
    const servedLabel = page.locator('#lblServed');
    const btnCoin100 = page.locator('#btnCoin100');
    const btnCoin25 = page.locator('#btnCoin25');
    const btnEspresso = page.locator('#btnEspresso');
    const btnMatcha = page.locator('#btnMatcha');
    const btnColdBrew = page.locator('#btnColdBrew');
    const btnRefund = page.locator('#btnRefund');

    // Initial: idle, $0.00, all drinks disabled
    await expect(stateLabel).toHaveText('idle');
    await expect(balanceLabel).toHaveText('$0.00');
    await expect(btnEspresso).toBeDisabled();
    await expect(btnMatcha).toBeDisabled();
    await expect(btnColdBrew).toBeDisabled();
    await expect(btnRefund).toBeDisabled();

    // Insert $1.00 + $0.25 + $0.25 -> $1.50 (Espresso enabled, Matcha disabled)
    await btnCoin100.click();
    await btnCoin25.click();
    await btnCoin25.click();
    await expect(balanceLabel).toHaveText('$1.50');
    await expect(btnRefund).toBeEnabled();
    await expect(btnEspresso).toBeEnabled();
    await expect(btnMatcha).toBeDisabled();

    // Select Espresso -> dispensing, then after 1.2s -> idle with $0.00 & served=1
    await btnEspresso.click();
    await expect(stateLabel).toHaveText('dispensing');
    await expect(balanceLabel).toHaveText('$0.00');
    await expect(servedLabel).toHaveText('1');
    await expect(stateLabel).toHaveText('idle', { timeout: 3000 });
  });

  test('Checkout Wizard', async ({ page }) => {
    await page.goto('/examples/checkout_wizard.html');

    const stateLabel = page.locator('#lblState');
    const itemsLabel = page.locator('#lblItems');
    const ordersLabel = page.locator('#lblOrders');
    const actionLog = page.locator('#actionLog');

    // Initial: cart
    await expect(stateLabel).toHaveText('cart');
    await expect(itemsLabel).toHaveText('1');
    await expect(actionLog).toContainText('entry(cart)');

    // Add item -> items=2
    await page.locator('#btnAddItem').click();
    await expect(itemsLabel).toHaveText('2');

    // Next -> payment (exit cart -> transition -> entry payment)
    await page.locator('#btnNext').click();
    await expect(stateLabel).toHaveText('payment');
    await expect(actionLog).toContainText('exit(cart)');
    await expect(actionLog).toContainText('entry(payment)');

    // Toggle card invalid + Submit -> validating always routes to paymentError
    await page.locator('#btnToggleCard').click();
    await page.locator('#btnSubmit').click();
    await expect(stateLabel).toHaveText('paymentError');

    // Retry -> payment, toggle card valid, Submit -> confirmed
    await page.locator('#btnRetry').click();
    await expect(stateLabel).toHaveText('payment');
    await page.locator('#btnToggleCard').click();
    await page.locator('#btnSubmit').click();
    await expect(stateLabel).toHaveText('confirmed');
    await expect(ordersLabel).toHaveText('1');
  });

  test('Autonomous Robot Vacuum', async ({ page }) => {
    await page.goto('/examples/robot_vacuum.html');

    const stateBadge = page.locator('#lblState');
    const wheelsPill = page.locator('#lblWheels');
    const vacuumPill = page.locator('#lblVacuum');
    const brushesPill = page.locator('#lblBrushes');

    const patrolNode = page.locator('#statechartSvg g.state-node[data-state-path="moving.cleaning.patrol"]');
    const spotNode = page.locator('#statechartSvg g.state-node[data-state-path="moving.cleaning.spot"]');
    const movingCluster = page.locator('#statechartSvg g.state-node[data-state-path="moving"]');
    const stairsNode = page.locator('#statechartSvg g.state-node[data-state-path="stopped.stairsAlert"]');
    const spotTransitionEdge = page.locator('#statechartSvg g.transition-edge[data-edge-source-path="moving.cleaning.patrol"][data-edge-event="SPOT_CLEAN"]');

    // Initial state: moving.cleaning.patrol
    await expect(stateBadge).toHaveText('moving.cleaning.patrol');
    await expect(wheelsPill).toHaveText('Driving Forward');
    await expect(vacuumPill).toHaveText('Standard');
    await expect(brushesPill).toHaveText('Spinning');
    await expect(movingCluster).toHaveClass(/is-active/);
    await expect(patrolNode).toHaveClass(/is-active/);
    await expect(spotTransitionEdge).toHaveClass(/is-enabled/);

    // Switch to Spot Clean by clicking the interactive transition edge directly in the SVG
    await spotTransitionEdge.click({ force: true });
    await expect(stateBadge).toHaveText('moving.cleaning.spot');
    await expect(vacuumPill).toHaveText('Turbo Boost');
    await expect(spotNode).toHaveClass(/is-active/);

    // Bump Furniture -> moving.avoidingObstacle, then after 1.2s -> returns to moving.cleaning.spot via History (H)
    await page.locator('#btnBump').click();
    await expect(stateBadge).toHaveText('moving.avoidingObstacle');
    await expect(wheelsPill).toHaveText('Reversing & Turning');
    await expect(stateBadge).toHaveText('moving.cleaning.spot', { timeout: 3000 });

    // Trigger Stairs -> exits Moving (shutting off all motors) and enters stopped.stairsAlert
    await page.locator('#btnStairs').click();
    await expect(stateBadge).toHaveText('stopped.stairsAlert');
    await expect(wheelsPill).toHaveText('Off');
    await expect(vacuumPill).toHaveText('Off');
    await expect(brushesPill).toHaveText('Off');
    await expect(stairsNode).toHaveClass(/is-active/);

    // Resume from Stopped -> returns to moving.cleaning.spot via History (H)
    await page.locator('#btnPauseResume').click();
    await expect(stateBadge).toHaveText('moving.cleaning.spot');
    await expect(vacuumPill).toHaveText('Turbo Boost');

    // Trigger Low Battery -> 'always' guard routes immediately to moving.returningToDock
    await page.locator('#btnLowBattery').click();
    await expect(stateBadge).toHaveText('moving.returningToDock');
    await expect(wheelsPill).toHaveText('Heading to Dock');

    // Flip between Diagram SVG and Machine JSON without distorting column widths
    const floorWrapper = page.locator('#floorWrapper');
    const widthBefore = (await floorWrapper.boundingBox()).width;
    const diagramView = page.locator('#diagramView');
    const jsonView = page.locator('#jsonView');
    await expect(diagramView).toBeVisible();
    await expect(jsonView).toBeHidden();

    await page.locator('#btnShowJson').click();
    await expect(jsonView).toBeVisible();
    await expect(diagramView).toBeHidden();
    await expect(jsonView).toContainText('"id": "robotVacuum"');
    const widthAfter = (await floorWrapper.boundingBox()).width;
    expect(Math.abs(widthAfter - widthBefore)).toBeLessThan(2);

    await page.locator('#btnShowDiagram').click();
    await expect(diagramView).toBeVisible();
    await expect(jsonView).toBeHidden();
  });

});
