#!/usr/bin/env python3
"""Generates docs/examples/crate_demo.glb: a 0.9 m cube (node "Box") with one
animation clip "work" (rotation about Y). Used by the autotest to check model
replacement + animations, and as a minimal reference of the expected format.
Usage: python3 docs/examples/make_demo_glb.py"""
import json, math, struct, pathlib

h = 0.45
# 24 vertices (4 per face) with normals.
faces = [((1,0,0),[(h,-h,-h),(h,h,-h),(h,h,h),(h,-h,h)]), ((-1,0,0),[(-h,-h,h),(-h,h,h),(-h,h,-h),(-h,-h,-h)]),
         ((0,1,0),[(-h,h,-h),(-h,h,h),(h,h,h),(h,h,-h)]), ((0,-1,0),[(-h,-h,h),(-h,-h,-h),(h,-h,-h),(h,-h,h)]),
         ((0,0,1),[(-h,-h,h),(h,-h,h),(h,h,h),(-h,h,h)]), ((0,0,-1),[(h,-h,-h),(-h,-h,-h),(-h,h,-h),(h,h,-h)])]
pos, nor, idx = [], [], []
for n, quad in faces:
    base = len(pos)
    pos += quad; nor += [n]*4
    idx += [base, base+1, base+2, base, base+2, base+3]
times = [0.0, 0.5, 1.0]
rots = [(0,math.sin(a/2),0,math.cos(a/2)) for a in (0.0, math.pi, 2*math.pi)]
blob = b"".join(struct.pack("<3f",*p) for p in pos)
off_n = len(blob); blob += b"".join(struct.pack("<3f",*n) for n in nor)
off_i = len(blob); blob += b"".join(struct.pack("<H",i) for i in idx)
blob += b"\0" * (-len(blob) % 4)
off_t = len(blob); blob += b"".join(struct.pack("<f",t) for t in times)
off_r = len(blob); blob += b"".join(struct.pack("<4f",*r) for r in rots)
gltf = {
 "asset":{"version":"2.0","generator":"aetherwright demo"},
 "scene":0,"scenes":[{"nodes":[0]}],
 "nodes":[{"name":"Box","mesh":0}],
 "meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1},"indices":2,"material":0}]}],
 "materials":[{"name":"Hologram","pbrMetallicRoughness":{"baseColorFactor":[0.3,0.85,1.0,1.0],"metallicFactor":0.2,"roughnessFactor":0.5},"emissiveFactor":[0.1,0.4,0.5]}],
 "buffers":[{"byteLength":len(blob)}],
 "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":off_n},{"buffer":0,"byteOffset":off_n,"byteLength":off_i-off_n},
   {"buffer":0,"byteOffset":off_i,"byteLength":len(idx)*2},{"buffer":0,"byteOffset":off_t,"byteLength":12},{"buffer":0,"byteOffset":off_r,"byteLength":48}],
 "accessors":[{"bufferView":0,"componentType":5126,"count":24,"type":"VEC3","min":[-h,-h,-h],"max":[h,h,h]},
   {"bufferView":1,"componentType":5126,"count":24,"type":"VEC3"},
   {"bufferView":2,"componentType":5123,"count":len(idx),"type":"SCALAR"},
   {"bufferView":3,"componentType":5126,"count":3,"type":"SCALAR","min":[0.0],"max":[1.0]},
   {"bufferView":4,"componentType":5126,"count":3,"type":"VEC4"}],
 "animations":[{"name":"work","samplers":[{"input":3,"output":4,"interpolation":"LINEAR"}],"channels":[{"sampler":0,"target":{"node":0,"path":"rotation"}}]}],
}
js = json.dumps(gltf, separators=(",",":")).encode()
js += b" " * (-len(js) % 4)
glb = struct.pack("<III", 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(blob))
glb += struct.pack("<II", len(js), 0x4E4F534A) + js + struct.pack("<II", len(blob), 0x004E4942) + blob
out = pathlib.Path(__file__).with_name("crate_demo.glb")
out.write_bytes(glb)
print(f"wrote {out} ({len(glb)} bytes)")
