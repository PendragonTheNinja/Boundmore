# Raptor model trial: Flow concept, then image to 3D

Goal: see whether an AI-made model reaches the quality we want, before
deciding how all the races are made. Claude takes the model from step 3 on.

## 1. Concept images in Google Flow

Make the **front view first**. When one looks right, make the side and back
views from it (in Flow, use the front image as the reference so the design
stays the same). Image-to-3D tools do best with several matching views, but
one good front view is enough to start.

**Front view**

> Full-body character concept art of a tall, lean, slightly hunched bird-reptile humanoid, standing in a neutral A-pose with arms held a little away from the body, facing the camera, centred, whole body in frame from head to feet, plain flat light grey background, soft even studio lighting, no shadows on the background. Stylised hand-painted fantasy game art, smooth sculpted forms, clean readable shapes, like a polished stylised MMO character.
> Body: long arms reaching to the knees, broad shoulders, narrow waist, wiry muscle. Small sleek head with a long tapering snout ending in a short hooked yellow beak, heavy brow ridges over fierce amber eyes with slit pupils. A crest of long swept-back pale grey blade-like feathers from the brow to the back of the head, continuing as a mane of pale spiky feathers around the neck and shoulders and a ridge of small spines down the back. Small feather tufts on the backs of the forearms.
> Skin: dark slate grey with subtle mottling, paler grey throat and chest. Legs bend backwards like a bird's, with yellow scaled shins and bird feet: three long forward toes and one back toe, each with a dark talon. Hands with three long fingers and a thumb, dark claws.
> Wears only a simple brown leather belt with a small brass buckle and a short cloth loincloth front and back.
> No weapons, no armour, no text, no logo, single character only.

**Side view** (with the front image as reference)

> The same character, exact same design and colours, seen from the side (profile, facing left), same A-pose, whole body in frame, plain flat light grey background, soft even lighting.

**Back view** (with the front image as reference)

> The same character, exact same design and colours, seen from directly behind, same A-pose, whole body in frame, plain flat light grey background, soft even lighting.

Tips:
- Keep the **plain background** and the **whole body in frame**; cropped feet or a busy background spoil the 3D result.
- The A-pose (arms a little out, legs straight) matters: arms against the body fuse into the torso in 3D.
- If Flow makes it too human or too cartoonish, add "creature, not human" or "not chibi, adult proportions".

Save the images in `art/model-trial/raptor/` (front.png, side.png, back.png) and push, so Claude can see them too.

## 2. Image to 3D

Try one of these (each has a free trial; check its terms before using a result in the game: free tiers often limit how a model may be used, paid tiers usually give you full rights):

- **Tripo** (tripo3d.ai): "Image to 3D", multi-view if offered (front, side, back), texture on, **no** auto-rig (Claude rigs it to the shared skeleton).
- **Meshy** (meshy.ai): "Image to 3D", art style "stylised" or "sculpture" if offered, texture on, quad/triangle either.
- **Rodin** (hyper3d.ai): "Image to 3D", multi-view, texture on.

Download as **GLB** (or FBX with textures if GLB is not offered) and put it in `art/model-trial/raptor/` beside the images, then push.

## 3. What Claude does with it

Clean the mesh, bring it to about 10,000 triangles, fit it to the shared
skeleton (`tools/bm-modelgen/kit.py`) so the same walk and idle play on it,
bake its detail into a normal map, mark the four colour areas (skin, belly,
markings, claws) so players can recolour it, and render it beside the scripted
raptor (`docs/media/m29-raptor-game.jpg`) for Nathan to judge.
