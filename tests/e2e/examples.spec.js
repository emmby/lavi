
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

  test('Simple Switch', async ({ page }) => {
    await page.goto('/examples/simple.html');
    
    const bulb = page.locator('#bulb');
    const stateLabel = page.locator('#lblState');
    const toggleBtn = page.locator('#btnToggle');

    // Initial State: OFF (bulb not .on)
    await expect(bulb).not.toHaveClass(/on/);
    await expect(stateLabel).toHaveText('OFF');

    // Toggle -> ON
    await toggleBtn.click();
    await expect(bulb).toHaveClass(/on/);
    await expect(stateLabel).toHaveText('ON');

    // Toggle -> OFF
    await toggleBtn.click();
    await expect(bulb).not.toHaveClass(/on/);
    await expect(stateLabel).toHaveText('OFF');
  });

  test('Traffic Light', async ({ page }) => {
    await page.goto('/examples/traffic_light.html');
    
    const red = page.locator('#red');
    const yellow = page.locator('#yellow');
    const green = page.locator('#green');
    const timerBtn = page.locator('#btnTimer');
    const stateLabel = page.locator('#lblState');
    const contextLabel = page.locator('#lblContext');

    // Initial State: GREEN
    await expect(green).toHaveClass(/active/);
    await expect(yellow).not.toHaveClass(/active/);
    await expect(red).not.toHaveClass(/active/);
    await expect(stateLabel).toHaveText('GREEN');
    await expect(contextLabel).toContainText('"cycle_count": 0');

    // Timer -> YELLOW
    await timerBtn.click();
    await expect(green).not.toHaveClass(/active/);
    await expect(yellow).toHaveClass(/active/);
    await expect(stateLabel).toHaveText('YELLOW');

    // Timer -> RED
    await timerBtn.click();
    await expect(yellow).not.toHaveClass(/active/);
    await expect(red).toHaveClass(/active/);
    await expect(stateLabel).toHaveText('RED');

    // Timer -> GREEN (cycle_count++ -> 1)
    await timerBtn.click();
    await expect(red).not.toHaveClass(/active/);
    await expect(green).toHaveClass(/active/);
    await expect(stateLabel).toHaveText('GREEN');
    await expect(contextLabel).toContainText('"cycle_count": 1');
  });

  test('Guards (Door)', async ({ page }) => {
    await page.goto('/examples/guards.html');

    const door = page.locator('#door');
    const openBtn = page.locator('#btnOpen');
    const closeBtn = page.locator('#btnClose');
    const lockBtn = page.locator('#btnLock');
    const unlockBtn = page.locator('#btnUnlock');
    const lockIcon = page.locator('#lockIcon');
    const stateLabel = page.locator('#lblState');

    // Initial: Closed & Locked
    await expect(door).not.toHaveClass(/open/);
    await expect(lockIcon).toHaveText('🔒'); 
    await expect(stateLabel).toHaveText('CLOSED');

    // Try Open (should fail because locked)
    await openBtn.click();
    await expect(door).not.toHaveClass(/open/);
    await expect(stateLabel).toHaveText('CLOSED');

    // Unlock
    await unlockBtn.click();
    await expect(lockIcon).toHaveText('🔓');

    // Open (should succeed)
    await openBtn.click();
    await expect(door).toHaveClass(/open/);
    await expect(stateLabel).toHaveText('OPEN');

    // Close
    await closeBtn.click();
    await expect(door).not.toHaveClass(/open/);
    await expect(stateLabel).toHaveText('CLOSED');
    
    // Lock again
    await lockBtn.click();
    await expect(lockIcon).toHaveText('🔒');
  });

  test('Actions Demo', async ({ page }) => {
    await page.goto('/examples/actions.html');

    const stateLabel = page.locator('#lblState');
    const contextLabel = page.locator('#lblContext');
    const nextBtn = page.locator('#btnNext');
    const resetBtn = page.locator('#btnReset');
    const logBox = page.locator('#logBox');

    // Initial: A, count=0
    await expect(stateLabel).toHaveText('A');
    await expect(stateLabel).toHaveClass(/state-a/);
    await expect(contextLabel).toContainText('"count": 0');

    // NEXT -> B, count=10 (from set_count_10) then incremented?
    // Let's check machine def:
    // A -> NEXT -> B: actions [log_transition, set_count_10]
    // B entry: [set_entered_b, increment, render]
    // So transition sets count to 10. Then entry increments it to 11.
    
    await nextBtn.click();
    
    await expect(stateLabel).toHaveText('B');
    await expect(stateLabel).toHaveClass(/state-b/);
    // Wait for context update
    await expect(contextLabel).toContainText('"count": 11'); 
    
    await expect(logBox).toContainText('Entered B');
    await expect(logBox).toContainText('Transitioning...');

    // RESET -> A
    // B -> RESET -> A: actions [increment, render]
    // A entry: [render]
    // So 11 -> 12.
    
    await resetBtn.click();
    await expect(stateLabel).toHaveText('A');
    await expect(contextLabel).toContainText('"count": 12');
    await expect(logBox).toContainText('Exited A'); // Note: 'Exited A' happens on exit of A, so that was in previous step log
  });
  
  test('Dynamic Assignment', async ({ page }) => {
      await page.goto('/examples/dynamic_assignment.html');
      
      const countLabel = page.locator('#counterVal');
      const incBtn = page.locator('#btnInc');
      const resetBtn = page.locator('#btnReset');

      // Initial: 0
      await expect(countLabel).toHaveText('0');

      // Inc -> 1
      await incBtn.click();
      await expect(countLabel).toHaveText('1');

      // Inc -> 2
      await incBtn.click();
      await expect(countLabel).toHaveText('2');
      
      // Reset -> 0
      await resetBtn.click();
      await expect(countLabel).toHaveText('0');
  });

  test('Parameterized Guards', async ({ page }) => {
      await page.goto('/examples/parameterized_guards.html');
      
      const stateLabel = page.locator('#state');
      const countLabel = page.locator('#count');
      const incBtn = page.locator('#btnInc');
      const nextBtn = page.locator('#btnNext');
      const resetBtn = page.locator('#btnReset');

      // Initial: start, 0
      await expect(stateLabel).toHaveText('start');
      await expect(countLabel).toHaveText('0');

      // Inc -> 1
      await incBtn.click();
      await expect(countLabel).toHaveText('1');
      
      // Try Next (1 > 2 is false) -> Stay start
      await nextBtn.click();
      await expect(stateLabel).toHaveText('start');

      // Inc -> 2
      await incBtn.click();
      await expect(countLabel).toHaveText('2');
      
      // Try Next (2 > 2 is false) -> Stay start
      await nextBtn.click();
      await expect(stateLabel).toHaveText('start');

      // Inc -> 3
      await incBtn.click();
      await expect(countLabel).toHaveText('3');

      // Try Next (3 > 2 is true) -> Medium
      await nextBtn.click();
      await expect(stateLabel).toHaveText('medium');

      // Reset -> Start, 0
      await resetBtn.click();
      await expect(stateLabel).toHaveText('start');
      await expect(countLabel).toHaveText('0');

      // Inc x 6 -> 6
      for(let i=0; i<6; i++) {
        await incBtn.click();
      }
      await expect(countLabel).toHaveText('6');

      // Try Next (6 > 5 is true) -> High
      await nextBtn.click();
      await expect(stateLabel).toHaveText('high');
      // Wait, in JS UI: document.getElementById('state').textContent = typeof snap.value === 'string' ? snap.value : JSON.stringify(snap.value);
      // If result is "high", prompt text is high.
      // But verify if `snap.value` is "high" or object. It should be "high".
      // Let's use regex or relax expectation.
      await expect(stateLabel).toHaveText(/high/);
  });

  test('Autonomous Robot Vacuum', async ({ page }) => {
    await page.goto('/examples/robot_vacuum.html');

    const stateBadge = page.locator('#lblState');
    const wheelsPill = page.locator('#lblWheels');
    const vacuumPill = page.locator('#lblVacuum');
    const brushesPill = page.locator('#lblBrushes');

    const patrolNode = page.locator('#state-patrol');
    const spotNode = page.locator('#state-spot');
    const movingCluster = page.locator('#state-moving');
    const stairsNode = page.locator('#state-stairsAlert');

    // Initial state: moving.cleaning.patrol
    await expect(stateBadge).toHaveText('moving.cleaning.patrol');
    await expect(wheelsPill).toHaveText('Driving Forward');
    await expect(vacuumPill).toHaveText('Standard');
    await expect(brushesPill).toHaveText('Spinning');
    await expect(movingCluster).toHaveClass(/active-parent/);
    await expect(patrolNode).toHaveClass(/active-leaf/);

    // Switch to Spot Clean -> moving.cleaning.spot
    await page.locator('#btnSpotClean').click();
    await expect(stateBadge).toHaveText('moving.cleaning.spot');
    await expect(vacuumPill).toHaveText('Turbo Boost');
    await expect(spotNode).toHaveClass(/active-leaf/);

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
    await expect(stairsNode).toHaveClass(/active-leaf/);

    // Flip between Diagram SVG and Machine JSON
    const diagramView = page.locator('#diagramView');
    const jsonView = page.locator('#jsonView');
    await expect(diagramView).toBeVisible();
    await expect(jsonView).toBeHidden();

    await page.locator('#btnShowJson').click();
    await expect(jsonView).toBeVisible();
    await expect(diagramView).toBeHidden();
    await expect(jsonView).toContainText('"id": "robotVacuum"');

    await page.locator('#btnShowDiagram').click();
    await expect(diagramView).toBeVisible();
    await expect(jsonView).toBeHidden();
  });

});
