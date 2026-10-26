"""Silent media fixtures for automated checks; never shipped as an app action."""
import json
import subprocess

def create_fixture(path):
    subprocess.run(['ffmpeg','-nostdin','-v','error','-y','-f','lavfi','-i',
        'testsrc2=size=320x180:rate=30','-f','lavfi','-i','anullsrc=r=48000:cl=stereo',
        '-t','3','-map','0:v','-map','1:a','-map','1:a','-c:v','libx264','-g','30',
        '-pix_fmt','yuv420p','-c:a','aac','-metadata:s:a:0','title=Game test','-metadata:s:a:1','title=Voice test',str(path)],check=True)
    return json.loads(subprocess.check_output(['ffprobe','-v','error','-show_streams',
        '-show_format','-of','json',str(path)]))
