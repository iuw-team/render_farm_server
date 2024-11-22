from __future__ import annotations
import bpy
import bpy.ops
import sys
import argparse
import urllib
import urllib.request
import requests
import json
import threading
import os
import code
from time import sleep

parser = argparse.ArgumentParser("blender_rendered")
parser.add_argument("--device", "-d", help="Select device: CUDA/OPENCL/OPTIX. ex `--device CUDA`")
parser.add_argument("--url", "-u", help="Url for server. ex `-u http://127.0.0.1:20`")

args = parser.parse_args()

PROJECT_FILE = "current_project.blend"
FRAME_FORMAT = 'PNG'
BASE_URL = args.url or "http://0.0.0.0:8080"


def set_settings():
    if args.device != None:
        bpy.context.preferences.addons["cycles"].preferences.compute_device_type = args.device

def open_project():
    bpy.ops.wm.open_mainfile(filepath=PROJECT_FILE)

def render_frame(frame_number: int):
    scene = bpy.context.scene
    fp = scene.render.filepath
    fp = './'

    scene.render.image_settings.file_format = FRAME_FORMAT
    scene.render.filepath = fp + str(frame_number)

    scene.frame_set(frame_number)
    bpy.ops.render.render(write_still=True)

    scene.render.filepath = fp

def get_devices():
    global devices
    bpy.context.preferences.addons["cycles"].preferences.get_devices()
    devices = bpy.context.preferences.addons["cycles"].preferences.devices

def turn_all_devices_on():
    bpy.context.scene.cycles.device = "GPU"
    for device in devices:
        device["use"] = 1

def download_project():
    try:
        urllib.request.urlretrieve(f"{BASE_URL}/frames", PROJECT_FILE)
    except Exception as e:
        print(f"Error downloading project from {BASE_URL}/frames")
        print(e)
        exit(1)

def get_frame_num():
    data = { "count": 1 }
    data = json.dumps(data).encode("utf-8")
    req = urllib.request.Request(f"{BASE_URL}/tasks", data)
    resp = urllib.request.urlopen(req)
    resp = json.loads(resp.read().decode('utf-8'))
    print(resp)

    global worker_id
    global lease_time
    global frames

    worker_id = resp['worker_id']
    lease_time = resp['lease_time']
    frames = resp['frames']

def submit_heartbeat(worker_id: int):
    req = urllib.request.Request(f"{BASE_URL}/tasks", method="POST")
    req.add_header('x-worker-id', worker_id)
    resp = urllib.request.urlopen(req)
    print("Sent keepalive")
    print(resp.msg)

def submit_frame(frame_num: int, frame_path: str):
    params = { "frame_id": frame_num }
    files = {"frame": open(frame_path, "rb")}
    print(params)
    print(f"worker_id: {worker_id}")
    global r
    global frames
    r = requests.put(f"{BASE_URL}/tasks", params = params, files=files, headers={"x-worker-id": worker_id})
    try: 
        print(r)
        print(r.reason)
    except Exception as e:
        print(e)

    #code.interact(local=locals())
    data = json.loads(r.text)

    assert(data['worker_id'] == worker_id)
    assert(len(data['frames']) == 1)
    new_frame = data['frames'][0]
    frames.append(new_frame)

def send_heartbeats():
    while True:
        sleep(60 * 5)
        submit_heartbeat(worker_id)

def start_heartbeating():
    thread = threading.Thread(target = send_heartbeats)
    thread.start()
    return thread


if __name__ == "__main__":
    download_project()
    open_project()
    get_devices()
    turn_all_devices_on()
    set_settings()

    get_frame_num()
    heartbeat_thread = start_heartbeating()
    while True:
        new_frame = frames.pop()
        print(f"Started rendering {new_frame}");
        render_frame(new_frame)
        filename = f"{new_frame}.png"
        submit_frame(new_frame, f"{new_frame}.png")
        os.remove(filename)

