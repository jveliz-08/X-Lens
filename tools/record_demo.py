#!/usr/bin/env python3
"""Record the ~2.5 minute demo video: a scripted walkthrough of the real dashboard, with a
synthetic hallway camera, captions and spoken narration (espeak-ng).

    runtime/venv/bin/python tools/record_demo.py [--live] [--lang en|es] [--out docs/demo.mp4]
    runtime/venv/bin/python tools/record_demo.py --serve      # live demo without hardware

Runs its own private server on 127.0.0.1:8099 (your real instance and data are untouched).
Without --live the AI is in simulation (the dashboard labels every such output SIMULATED). With --live it uses the key in runtime/meta.secret: about ten budgeted Meta calls
(event captions, Ask, daily summary, SAM 3.1). Needs Playwright's Chromium, Pillow for the
system python3 and ffmpeg. Narration uses Piper voices from runtime/voices when present
(neural, offline), else espeak-ng. --serve only starts the private server and synthetic
camera for presenting by hand: press Enter to make someone walk in.
"""
import argparse
import base64
import json
import os
import pathlib
import secrets
import shutil
import subprocess
import tempfile
import threading
import time
import urllib.request
import sys
import wave
from playwright.sync_api import sync_playwright, expect

ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tools'))
import vision
BIN=ROOT/'target/release/x-lens'
PORT=8099
URL=f'http://127.0.0.1:{PORT}'
DEVICE='hallway'

SCRIPT={
 'en':{
  'problem':'Millions of people live alone, and their families worry. Smart cameras either stream everything to someone else\'s cloud, or send every frame to an A.I. X-Lens does neither.',
  'request':'This is the exact request X-Lens sends to Muse Spark for that event: three frames, your watches, a reference frame you labeled, and a strict JSON schema. Anything outside the schema is rejected.',
  'hardware':'It runs on a real ESP32-CAM: twenty eight milliseconds of motion detection on the chip, five to seven frames per second while watched, one per second when idle, and a Rust host with zero dependencies.',
  'title':('X-Lens. A seven dollar camera that understands, and only pays for A.I. when something happens.'),
  'live':'This is the live feed of a small camera board. It measures motion on the chip itself, and streams at full rate only while someone is watching.',
  'event':'Someone walks in. The camera detects the motion, and the host records a clip that includes the seconds before it started.',
  'activity':'Muse Spark looks at three frames of the event, once, and writes what happened. One model call per event, never per frame.',
  'ask':'Ask your cameras in plain words, by voice or text. Answers cite the clips. Ask to be alerted, and one click turns it into a watch.',
  'learn':'Watches learn without training. Mark an alert as right, or as a false alarm, and that frame becomes a reference example in the next call.',
  'routines':'For someone who lives alone, the absence of activity is the signal. Routines expect motion in a time window, and alert you when nothing moves. No A.I. needed.',
  'digest':'At night, one call turns the whole day into a short story, with links to every clip.',
  'zones':'A tree or a TV that keeps moving? Paint it out, or let SAM three point one find it. The camera applies the mask itself.',
  'close':'X-Lens. Your footage stays home. The A.I. describes and suggests, never acts on its own, and your family stays looked after.',
  'ask_text':'Alert me if a package is left at the door',
  'routine_ok':'Grandma gets up','routine_missed':'Grandpa takes his morning walk','concept':'window',
 },
 'es':{
  'problem':'Millones de personas viven solas, y sus familias se preocupan. Las cámaras inteligentes mandan todo a la nube de otro, o envían cada imagen a una inteligencia artificial. X-Lens no hace ninguna de las dos.',
  'request':'Esta es la petición exacta que X-Lens envía a Muse Spark por ese evento: tres imágenes, tus vigilancias, una imagen de referencia que etiquetaste, y un esquema JSON estricto. Todo lo que salga del esquema se rechaza.',
  'hardware':'Funciona en una ESP32-CAM real: veintiocho milisegundos de detección de movimiento en el chip, cinco a siete imágenes por segundo cuando la miras, una por segundo en reposo, y un servidor en Rust sin dependencias.',
  'title':'X-Lens. Una cámara de siete dólares que entiende, y solo paga por inteligencia artificial cuando algo pasa.',
  'live':'Este es el video en vivo de una pequeña placa con cámara. Mide el movimiento en el propio chip, y transmite a máxima velocidad solo cuando alguien está mirando.',
  'event':'Alguien entra. La cámara detecta el movimiento, y el servidor graba un clip que incluye los segundos anteriores.',
  'activity':'Muse Spark mira tres imágenes del evento, una sola vez, y escribe lo que pasó. Una llamada por evento, nunca por imagen.',
  'ask':'Pregúntale a tus cámaras con palabras simples, por voz o texto. Las respuestas citan los clips. Pide una alerta, y con un clic se convierte en una vigilancia.',
  'learn':'Las vigilancias aprenden sin entrenamiento. Marca una alerta como correcta, o como falsa alarma, y esa imagen se vuelve un ejemplo para la siguiente llamada.',
  'routines':'Para alguien que vive solo, la falta de actividad es la señal. Las rutinas esperan movimiento en un horario, y te avisan si nada se mueve. Sin inteligencia artificial.',
  'digest':'En la noche, una llamada convierte el día completo en una historia corta, con enlaces a cada clip.',
  'zones':'¿Un árbol o una tele que no paran de moverse? Píntalos, o deja que SAM tres punto uno los encuentre. La cámara aplica la máscara ella misma.',
  'close':'X-Lens. Tus videos se quedan en casa. La inteligencia artificial describe y sugiere, nunca actúa por su cuenta, y tu familia queda acompañada.',
  'ask_text':'Avísame si dejan un paquete en la puerta',
  'routine_ok':'La abuela se levanta','routine_missed':'El abuelo sale a caminar','concept':'ventana',
 }}
VOICE={'en':'en-us','es':'es-419'}
PIPER={'en':'en_US-ryan-high','es':'es_MX-ald-medium'}

OVERLAY='''
(()=>{const css=`#rec-cap{position:fixed;left:50%;bottom:28px;transform:translateX(-50%);z-index:99;max-width:min(1040px,92vw);padding:12px 20px;border-radius:12px;
 background:rgba(0,0,0,.78);color:#fff;font:600 21px/1.45 Inter,system-ui,sans-serif;text-align:center;box-shadow:0 8px 30px rgba(0,0,0,.5)}
#rec-card{position:fixed;inset:0;z-index:98;display:grid;place-content:center;gap:14px;text-align:center;background:radial-gradient(circle at 50% 40%,#18210f,#0b0e11 70%);color:#e8edf1;font-family:Inter,system-ui,sans-serif}
#rec-card h1{font-size:64px;margin:0;letter-spacing:-1.5px}#rec-card p{font-size:24px;margin:0;color:#c5f095}#rec-card small{font-size:17px;color:#8d99a5}
#rec-card pre{margin:6px auto 0;max-width:1120px;text-align:left;font:15px/1.45 ui-monospace,Menlo,Consolas,monospace;color:#dfe7ee;background:#0f1418;border:1px solid #323b45;border-radius:12px;padding:16px 20px;white-space:pre-wrap}
#rec-card h2{font-size:30px;margin:0}
#rec-mark{position:fixed;top:10px;right:12px;z-index:99;padding:4px 10px;border-radius:999px;background:rgba(245,184,75,.92);color:#1d1a12;font:700 12px Inter,system-ui,sans-serif;letter-spacing:.04em}`;
 const add=()=>{if(document.getElementById('rec-style'))return;const s=document.createElement('style');s.id='rec-style';s.textContent=css;document.head.append(s);};
 window.__cap=t=>{add();let e=document.getElementById('rec-cap');if(!e){e=document.createElement('div');e.id='rec-cap';document.body.append(e);}e.textContent=t;e.hidden=!t;};
 window.__card=(h,p,small)=>{add();let e=document.getElementById('rec-card');if(!e){e=document.createElement('div');e.id='rec-card';document.body.append(e);}
  e.replaceChildren();for(const [tag,v] of [['h1',h],['p',p],['small',small]])if(v){const x=document.createElement(tag);x.textContent=v;e.append(x);}e.hidden=false;};
 window.__code=(h,code)=>{window.__card();const e=document.getElementById('rec-card');const t=document.createElement('h2');t.textContent=h;const pre=document.createElement('pre');pre.textContent=code;e.replaceChildren(t,pre);e.hidden=false;};
 window.__uncard=()=>{const e=document.getElementById('rec-card');if(e)e.hidden=true;};
 window.__mark=t=>{add();let e=document.getElementById('rec-mark');if(!e){e=document.createElement('div');e.id='rec-mark';document.body.append(e);}e.textContent=t;};
})();'''

class Camera(threading.Thread):
    """Uploads the synthetic hallway like a real board: ~4 fps idle, 8 fps while something moves."""
    def __init__(self,scene,key):
        super().__init__(daemon=True)
        self.scene,self.key=scene,key
        self.walk=[(scene/f'walk-{i:03d}.jpg').read_bytes() for i in range(len(json.loads((scene/'motion.json').read_text())['walk']))]
        self.motion=json.loads((scene/'motion.json').read_text())['walk']
        self.idle,self.after=(scene/'idle.jpg').read_bytes(),(scene/'after.jpg').read_bytes()
        self.episode=threading.Event();self.done=threading.Event();self.stopped=False;self.box=False;self.seq=1
    def post(self,jpeg,motion):
        r=urllib.request.Request(f'{URL}/api/frame/{DEVICE}',jpeg,{'Authorization':'Bearer '+self.key,'Content-Type':'image/jpeg',
            'X-Sequence':str(self.seq),'X-Protocol':'1','X-Motion':str(motion)},method='POST')
        self.seq+=1
        try: urllib.request.urlopen(r,timeout=5).read()
        except OSError: pass
    def run(self):
        while not self.stopped:
            if self.episode.is_set():
                for jpeg,m in zip(self.walk,self.motion):
                    self.post(jpeg,m);time.sleep(1/8)
                self.box=True;self.episode.clear();self.done.set()
            else:
                self.post(self.after if self.box else self.idle,0);time.sleep(.25)

KOKORO={'en':('af_heart','en-us'),'es':('ef_dora','es')}
_kokoro=[]
def narration(text,path,lang):
    """Kokoro (natural, offline) if its model is in runtime/voices, else Piper, else espeak-ng."""
    voices=ROOT/'runtime/voices'
    if (voices/'kokoro-v1.0.onnx').is_file() and (voices/'voices-v1.0.bin').is_file():
        try:
            from kokoro_onnx import Kokoro
            import soundfile
            if not _kokoro: _kokoro.append(Kokoro(str(voices/'kokoro-v1.0.onnx'),str(voices/'voices-v1.0.bin')))
            voice,code=KOKORO[lang]
            audio,rate=_kokoro[0].create(text,voice=voice,speed=1.0,lang=code)
            soundfile.write(str(path),audio,rate,subtype='PCM_16')
            return len(audio)/rate
        except ImportError: pass
    model=ROOT/f'runtime/voices/{PIPER[lang]}.onnx'
    piper=pathlib.Path(sys.executable).with_name('piper')
    if model.is_file() and piper.is_file():
        subprocess.run([str(piper),'-m',str(model),'-f',str(path),'--sentence-silence','0.25','--length-scale',{'en':'1.0','es':'0.86'}[lang]],input=text.encode(),check=True,
                       stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    else: subprocess.run(['espeak-ng','-v',VOICE[lang],'-s','150','-w',str(path),text],check=True)
    with wave.open(str(path)) as w: return w.getnframes()/w.getframerate()

def request_preview(scene,watch,language):
    """The real Muse Spark request for the recorded event (vision.event_body), images elided."""
    frames=[base64.b64encode((scene/f'walk-{i:03d}.jpg').read_bytes()).decode() for i in (0,15,30)]
    v={'device':DEVICE,'seconds':8,'peak':132,'language':language,'frames':frames,'watches':[{'id':'watch-1','prompt':watch}],
       'examples':[{'watch':'watch-1','match':False,'frame':base64.b64encode((scene/'idle.jpg').read_bytes()).decode()}]}
    body=json.loads(vision.event_body('muse-spark-1.3',v)[0])
    out=[f'POST https://api.meta.ai/v1/chat/completions   model: {body["model"]}   reasoning_effort: {body["reasoning_effort"]}']
    out.append('developer: '+body['messages'][0]['content'][:150]+'…')
    for part in body['messages'][1]['content']:
        if part['type']=='image_url': out.append(f'user  ▣ image  data:image/jpeg;base64 ({len(part["image_url"]["url"])*3//4//1024} KB)')
        else: out.append('user  '+part['text'].replace('\n',' ')[:118])
    schema=body['response_format']['json_schema']
    props=schema['schema']['properties']
    out.append(f'response_format: json_schema "{schema["name"]}" strict={str(schema["strict"]).lower()}')
    out.append('  caption: string · activity: enum('+', '.join(props['activity']['enum'][:5])+', …) · watches[{id: enum(watch-1), match, confidence, reason}]')
    return '\n'.join(out)

def main():
    p=argparse.ArgumentParser(description=__doc__,formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('--live',action='store_true',help='Use runtime/meta.secret (spends about ten Meta calls)')
    p.add_argument('--lang',choices=['en','es'],default='en')
    p.add_argument('--out',default=str(ROOT/'docs/demo.mp4'))
    p.add_argument('--seconds',type=float,default=180,help='Target video length (the hackathon limit is 3 minutes)')
    p.add_argument('--serve',action='store_true',help='Only run the private server and synthetic camera for a live presentation')
    args=p.parse_args()
    lines=SCRIPT[args.lang]
    key_file=ROOT/'runtime/meta.secret'
    if args.live and not key_file.is_file(): p.error('--live needs your key in runtime/meta.secret (save it from the dashboard first)')
    chrome=os.environ.get('CHROMIUM_PATH') or str(next(pathlib.Path.home().glob('.cache/ms-playwright/chromium-*/chrome-linux/chrome')))
    with tempfile.TemporaryDirectory() as temp:
        tmp=pathlib.Path(temp);os.chmod(tmp,0o700)
        subprocess.run([sys.executable,str(ROOT/'tools/demo_scene.py'),str(tmp/'scene')],check=True)
        viewer,operator,device=(secrets.token_hex(32) for _ in range(3))
        cfg=tmp/'config';cfg.write_text(f'viewer={viewer}\noperator={operator}\ndevice.{DEVICE}={device}\n');cfg.chmod(0o600)
        env={**os.environ,'XLENS_PORT':str(PORT),'XLENS_AI_STATE_DIR':temp}
        if args.live: env['XLENS_META_KEY_FILE']=str(key_file);env.pop('XLENS_RULE_MODE',None)
        else: env['XLENS_RULE_MODE']='mock'
        server=subprocess.Popen([str(BIN),str(cfg),str(tmp/'data')],cwd=ROOT,env=env,stderr=subprocess.DEVNULL)
        cam=Camera(tmp/'scene',device)
        segments=[]
        try:
            for _ in range(100):
                try: urllib.request.urlopen(URL+'/',timeout=1).read();break
                except OSError: time.sleep(.05)
            cam.start()
            if args.lang=='es': # Captions in the narration language too.
                urllib.request.urlopen(urllib.request.Request(URL+'/api/ai-config',b'language=es\nauto=1',{'Authorization':'Bearer '+operator,'X-Protocol':'1','Content-Type':'text/plain'},method='POST'),timeout=5)
            if args.serve:
                print(f'Dashboard: {URL}\nViewer token:   {viewer}\nOperator token: {operator}\nAI: {"LIVE Meta" if args.live else "simulation"}. '
                      'Press Enter to make someone walk in; Ctrl+C to stop (everything is deleted).')
                try:
                    while True:
                        input();cam.box=False;cam.done.clear();cam.episode.set();print('Someone walks in…')
                except (KeyboardInterrupt,EOFError): return
            # Narration first: the video starts recording as soon as the page exists.
            order=['title','problem','live','event','activity','request','ask','learn','routines','digest','zones','hardware','close']
            voiced={n:(tmp/f'{i:02d}-{n}.wav',narration(lines[n],tmp/f'{i:02d}-{n}.wav',args.lang)) for i,n in enumerate(order)}
            with sync_playwright() as pw:
                browser=pw.chromium.launch(executable_path=chrome,headless=True,args=['--no-sandbox'])
                ctx=browser.new_context(bypass_csp=True,viewport={'width':1280,'height':720}, # Only for the recorder's caption overlay.
                                       record_video_dir=str(tmp/'video'),record_video_size={'width':1280,'height':720})
                page=ctx.new_page();start=time.monotonic()
                page.add_init_script(OVERLAY)
                page.goto(URL)
                def scene(name,action=lambda:None,card=None):
                    at=time.monotonic()-start;wav,length=voiced[name]
                    segments.append((at,wav))
                    # Spread the time left until --seconds over the remaining scenes (≈2 s of UI work each).
                    rest_names=order[order.index(name):]
                    spare=args.seconds-at-sum(voiced[n][1] for n in rest_names)-2.0*len(rest_names)-1.5
                    pause=max(.7,min(6,spare/len(rest_names)))
                    if card: page.evaluate("a=>window.__card(...a)",card)
                    page.evaluate("t=>window.__cap(t)",lines[name])
                    t0=time.monotonic();action()
                    rest=length+pause-(time.monotonic()-t0)
                    if rest>0: page.wait_for_timeout(rest*1000)
                def tab(name): page.locator(f'nav [data-tab={name}]').click()
                scene('title',card=('X-Lens','A $7 camera that understands.','Meta Muse Spark · SAM 3.1 · Muse Voice Transcribe — edge motion, self-hosted clips'))
                scene('problem',card=('Living alone shouldn\'t mean unseen.','Cloud cameras stream everything away. AI cameras pay for every frame.','X-Lens: motion on the chip · clips at home · one Meta call per event'))
                def unlock():
                    page.evaluate('window.__uncard()');page.locator('#token').press_sequentially(viewer[:12],delay=8);page.locator('#token').fill(viewer)
                    page.locator('#login-operator').fill(operator);page.locator('#connect').click()
                    page.wait_for_function("()=>document.querySelector('.camera img')?.naturalWidth===320",timeout=15000)
                scene('live',unlock)
                def event():
                    cam.episode.set();page.wait_for_selector('.camera.recording',timeout=15000)
                scene('event',event)
                cam.done.wait(15)
                def activity():
                    tab('activity')
                    page.wait_for_function("()=>{const c=document.querySelector('.event .caption');return c&&!c.classList.contains('muted')}",timeout=40000)
                    page.locator('.event .thumb').first.click();page.wait_for_timeout(3500);page.locator('#player-close').click()
                scene('activity',activity)
                scene('request',lambda:page.evaluate("a=>window.__code(...a)",['What Muse Spark receives (generated by tools/vision.py, images elided)',
                      request_preview(tmp/'scene',{'en':'A package is left at the door','es':'Dejan un paquete en la puerta'}[args.lang],args.lang)]))
                page.evaluate('window.__uncard()')
                def ask():
                    tab('overview');page.locator('#ask-input').press_sequentially(lines['ask_text'],delay=28);page.locator('#ask-send').click()
                    page.wait_for_selector('#ask-answer .answer',timeout=40000)
                    make=page.locator('#ask-answer button.primary')
                    if make.count():page.wait_for_timeout(1200);make.click()
                scene('ask',ask)
                def learn():
                    tab('activity');page.locator('#refresh').evaluate('b=>b.click()')
                    teach=page.locator('.teach button').first
                    try: teach.wait_for(timeout=8000);page.wait_for_timeout(1200);teach.click()
                    except Exception: pass
                scene('learn',learn)
                def routines():
                    now=time.localtime();m=now.tm_hour*60+now.tm_min
                    tab('rules');page.locator('#routine-label').scroll_into_view_if_needed()
                    for label,a,b in [(lines['routine_ok'],max(0,m-90),min(1439,m+60)),(lines['routine_missed'],m-180,m-120)]:
                        if a<0 or b<=a: continue
                        page.locator('#routine-label').press_sequentially(label,delay=25)
                        page.locator('#routine-from').fill(f'{a//60:02d}:{a%60:02d}');page.locator('#routine-to').fill(f'{b//60:02d}:{b%60:02d}')
                        page.locator('#routine-save').click();page.wait_for_timeout(900)
                    tab('activity');page.wait_for_function("()=>document.querySelector('#routine-status')?.textContent.includes(':')",timeout=12000)
                scene('routines',routines)
                def digest():
                    page.locator('#digest-make').click();page.wait_for_selector('#digest h3',timeout=40000)
                scene('digest',digest)
                def zones():
                    tab('overview');page.locator('.camera-bar button',has_text='Zones').first.click()
                    page.wait_for_function("()=>document.querySelector('#zones-img').naturalWidth===320")
                    for cell in [8,9,10,16,17,18]: page.locator('#zones-grid button').nth(cell).click();page.wait_for_timeout(150)
                    page.locator('#zones-concept').press_sequentially(lines['concept'],delay=40);page.locator('#zones-detect').click()
                    page.wait_for_timeout(2500);page.locator('#zones-save').click();page.wait_for_timeout(1500);page.locator('#zones-close').click()
                scene('zones',zones)
                scene('hardware',card=('Real hardware','ESP32-CAM · OV3660 · on-chip JPEG decode at 1/8 scale: 28 ms motion score per frame',
                    '5–7 fps while watched · ~1 fps idle · self-healing Wi-Fi · zero-dependency Rust host · 11 + 19 + 37 tests + browser workflow'))
                scene('close',card=('X-Lens','Your footage stays home. The AI describes and suggests — it never acts.','One Meta call per event · watches that learn · routines for loved ones · ask by voice'))
                page.wait_for_timeout(max(1500,(args.seconds-(time.monotonic()-start))*1000))
                video=page.video.path();ctx.close();browser.close()
            out=pathlib.Path(args.out);out.parent.mkdir(parents=True,exist_ok=True)
            inputs=['-i',str(video)];delays=[]
            for i,(at,wav) in enumerate(segments):
                inputs+=['-i',str(wav)];ms=int(at*1000);delays.append(f'[{i+1}]adelay={ms}|{ms}[a{i}]')
            mix=';'.join(delays)+';'+''.join(f'[a{i}]' for i in range(len(segments)))+f'amix=inputs={len(segments)}:duration=longest:normalize=0,apad[aout]'
            subprocess.run(['ffmpeg','-y','-loglevel','error',*inputs,'-filter_complex',mix,'-map','0:v','-map','[aout]','-vf','tpad=stop_mode=clone:stop_duration=3','-t',str(args.seconds),'-c:v','libx264','-pix_fmt','yuv420p',
                            '-crf','23','-preset','medium','-c:a','aac','-b:a','128k','-movflags','+faststart',str(out)],check=True)
            print(f'Wrote {out} ({out.stat().st_size//1024} KiB, {"live Meta" if args.live else "AI in simulation"}).')
        finally:
            cam.stopped=True;server.terminate();server.wait(timeout=5)

if __name__=='__main__': main()
