#!/usr/bin/env python3
"""Optional dev-only smoke test; requires Playwright and system Chromium."""
import pathlib
import subprocess
import tempfile
import time
from playwright.sync_api import sync_playwright
from integration import BIN,VIEW,DEVICE,send,req
with tempfile.TemporaryDirectory() as temp:
    root=pathlib.Path(temp); cfg=root/'config'; cfg.write_text(f'viewer={VIEW}\ndevice.demo={DEVICE}\n'); cfg.chmod(0o600)
    proc=subprocess.Popen([str(BIN),str(cfg),str(root/'data')],stderr=subprocess.DEVNULL)
    try:
        for _ in range(100):
            if proc.poll() is not None: raise RuntimeError('Server failed')
            try: req('GET','/'); break
            except OSError: time.sleep(.02)
        assert send(1)==201
        with sync_playwright() as p:
            browser=p.chromium.launch(executable_path='/usr/bin/chromium',headless=True,args=['--no-sandbox'])
            page=browser.new_page(viewport={'width':1280,'height':1000})
            errors=[]; page.on('pageerror',lambda e:errors.append(str(e)))
            page.goto('http://127.0.0.1:8080')
            assert page.locator('#login').is_visible()
            page.locator('#token').fill(VIEW); page.locator('#connect').click()
            page.locator('.camera img').wait_for()
            page.wait_for_function("document.querySelector('.camera img').naturalWidth === 320")
            assert page.locator('.camera h3').inner_text()=='demo'
            assert page.evaluate('localStorage.length')==0
            assert page.evaluate('sessionStorage.length')==0
            page.screenshot(path=str(pathlib.Path(__file__).resolve().parents[1]/'docs/dashboard.png'),full_page=True)
            page.set_viewport_size({'width':390,'height':844})
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
            page.locator('#lock').click()
            assert page.locator('#login').is_visible()
            assert page.locator('.camera').count()==0
            assert page.locator('#token').input_value()==''
            assert not errors,errors
            browser.close()
        print('Browser smoke passed: login, JPEG decode, mobile layout, no persistent token, lock, no JS errors')
    finally: proc.terminate(); proc.wait(timeout=5)
