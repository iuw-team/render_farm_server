from __future__ import annotations
import bpy
import bpy.ops

PROJECT_FILE = "test_render.blend"
FRAME_FORMAT = 'PNG'

bpy.ops.wm.open_mainfile(filepath=PROJECT_FILE)

def render_frame(frame_number: int):
    scene = bpy.context.scene
    fp = scene.render.filepath

    scene.render.image_settings.file_format = FRAME_FORMAT
    scene.render.filepath = fp + str(frame_number)
    bpy.ops.render.render(write_still=True)

    scene.render.filepath = fp

def get_devices():
    global devices
    bpy.context.preferences.addons["cycles"].preferences.get_devices()
    devices = bpy.context.preferences.addons["cycles"].preferences.devices

print("Rendering frame 1");
render_frame(1)

get_devices()
print(devices)
