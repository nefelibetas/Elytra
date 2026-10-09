// 自研等距 3D 渲染器：正交投影 + 画家算法 + Minecraft 公开皮肤 UV 规范。
//
// 零依赖（只用 Canvas 2D），替代 three.js / skinview3d —— 启动器只需要「正脸头像」与
// 「全身预览」两种小图，真透视相机 + 光照 + 后处理是纯粹的浪费。
//
// 与真 3D 的差别（也是选它的理由）：
//   - **正交投影**：yaw/pitch 直接代进旋转矩阵，没有任何相机状态可被覆盖或残留，
//     同一组参数永远得到同一张图（真 3D 那套「相机被库内部重算」的坑不存在）。
//   - **正交下四边形保持平行四边形** → 每个面一个仿射矩阵即可贴图，不必拆三角形做透视校正。
//   - **贴合裁切**：画布按可见顶点的投影包围盒自适应，头像天然居中填满、不会跑偏。
//
// 皮肤布局是 Mojang 公开规范（64×64；旧版 64×32 亦可）：立方体 UV 展开从左到右依次是
// [深度][正面][深度][背面]，上面/下面贴在右上角。公开格式文档，不是任何服务的私有算法。

/** 立方体六面在 UV 展开图上的取样矩形。 */
type UvRect = readonly [number, number, number, number];

/** 一个待绘制的体素立方体：中心 + 尺寸（1 体素 = 皮肤 1 像素）。 */
interface VoxelBox {
  cx: number;
  cy: number;
  cz: number;
  w: number;
  h: number;
  d: number;
  u: number;
  v: number;
  /** 第二层（帽子/外套/袖腿）向外膨胀的体素量；0 = 第一层 */
  inflate: number;
  /** 贴图水平镜像（64×32 旧格式的左侧肢体复用右侧贴图时用） */
  mirror?: boolean;
  /** 绕肩/髋的 X 轴摆动（度）：正 = 向后摆。用于待机/行走动画。 */
  swing?: number;
  /** 绕支点的 Z 轴摆动（度）：正 = 下端向 +x。用于手臂外张（skinview3d 的 idle/walk）。 */
  swingZ?: number;
  /** 绕支点的 Y 轴旋转（度）：正 = 向 +x 侧偏转。用于行走的头部摆动（headBobbing）。 */
  swingY?: number;
  /** 摆动支点（体素坐标，三轴共用）；未给则绕立方体中心。 */
  pivot?: readonly [number, number, number];
  /** 用披风贴图而非皮肤贴图绘制（披风是独立的 64×32 纹理，UV 原点同为 (0,0)）。 */
  cape?: boolean;
  /** 交换 z 方向两面的贴图区域。
   *  皮肤/身体靠它保持「正面朝观察者」；披风贴在背后、朝外的面是 -z，
   *  不交换的话图案会朝到身体那侧（背面看只有素色）。 */
  flipZ?: boolean;
  /** UV 布局尺寸（缺省 = 几何尺寸）。
   *  鞘翅是典型例子：几何为 12×22×4（基础盒 10×20×2 加 1 体素膨胀），
   *  但贴图按基础盒 10×20×2 的展开切分（MC 官方模型即如此），两者不等。 */
  uvW?: number;
  uvH?: number;
  uvD?: number;
  /** 不剔背面（两面都画）。
   *  鞘翅用：翅膀贴图只画在**朝外那一面**（另一面全透明），剔除背面会让翅膀从正面完全消失。
   *  MC 对应 `RenderType.armorCutoutNoCull`，即鞘翅是不做背面剔除的。 */
  noCull?: boolean;
}

/** 一个面的静态定义：法线 + UV 取样矩形 + 四顶点（从面外看的左下、右下、右上、左上）。 */
interface FaceDef {
  normal: readonly [number, number, number];
  uv: UvRect;
  /** 顺序固定为：左下、右下、右上、左上（从面外看）；与贴图左下角对齐 */
  corners: readonly (readonly [number, number, number])[];
}

/** 立方体局部坐标下的四角（从面外看的左下、右下、右上、左上），已含膨胀。 */
function cornersFor(
  axis: 0 | 1 | 2,
  sign: 1 | -1,
  hw: number,
  hh: number,
  hd: number,
): readonly (readonly [number, number, number])[] {
  // 每个面自成一个「右向量 / 上向量」坐标系，保证贴上来的贴图是正立的。
  if (axis === 0) {
    // ±X 面：水平轴 = ∓Z，竖直轴 = +Y
    const s = sign;
    return [
      [s * hw, -hh, s * hd],
      [s * hw, -hh, -s * hd],
      [s * hw, hh, -s * hd],
      [s * hw, hh, s * hd],
    ];
  }
  if (axis === 1) {
    // ±Y 面：水平轴 = +X，竖直轴 = ∓Z
    const s = sign;
    return [
      [-hw, s * hh, -s * hd],
      [hw, s * hh, -s * hd],
      [hw, s * hh, s * hd],
      [-hw, s * hh, s * hd],
    ];
  }
  // ±Z 面：水平轴 = ±X，竖直轴 = +Y
  const s = sign;
  return [
    [-s * hw, -hh, s * hd],
    [s * hw, -hh, s * hd],
    [s * hw, hh, s * hd],
    [-s * hw, hh, s * hd],
  ];
}

/**
 * 立方体六面。
 *
 * 按 Mojang 公开布局，以 (u,v) 为立方体 UV 左上角：
 *   上面 (u+d, v)        尺寸 (w, d)
 *   下面 (u+d+w, v)      尺寸 (w, d)
 *   左面 (u, v+d)        尺寸 (d, h)
 *   正面 (u+d, v+d)      尺寸 (w, h)
 *   右面 (u+d+w, v+d)    尺寸 (d, h)
 *   背面 (u+d+w+d, v+d)  尺寸 (w, h)
 */
function facesOf(b: VoxelBox): FaceDef[] {
  const { u, v } = b;
  // UV 布局尺寸可与几何尺寸不同（见 VoxelBox.uvW 的说明）；几何半宽用几何尺寸算
  const uvw = b.uvW ?? b.w;
  const uvh = b.uvH ?? b.h;
  const uvd = b.uvD ?? b.d;
  const hw = b.w / 2 + b.inflate;
  const hh = b.h / 2 + b.inflate;
  const hd = b.d / 2 + b.inflate;
  // z 方向两面的贴图区域：默认 +z 用 north(u+d)、-z 用 south(u+d+w+d)；
  // flipZ 时对调（披风朝外的面是 -z，要显示图案所在的 north）
  const uvZPlus: UvRect = b.flipZ
    ? [u + uvd + uvw + uvd, v + uvd, uvw, uvh]
    : [u + uvd, v + uvd, uvw, uvh];
  const uvZMinus: UvRect = b.flipZ
    ? [u + uvd, v + uvd, uvw, uvh]
    : [u + uvd + uvw + uvd, v + uvd, uvw, uvh];
  return [
    { normal: [0, 1, 0], uv: [u + uvd, v, uvw, uvd], corners: cornersFor(1, 1, hw, hh, hd) },
    { normal: [0, -1, 0], uv: [u + uvd + uvw, v, uvw, uvd], corners: cornersFor(1, -1, hw, hh, hd) },
    { normal: [-1, 0, 0], uv: [u, v + uvd, uvd, uvh], corners: cornersFor(0, -1, hw, hh, hd) },
    { normal: [0, 0, 1], uv: uvZPlus, corners: cornersFor(2, 1, hw, hh, hd) },
    {
      normal: [1, 0, 0],
      uv: [u + uvd + uvw, v + uvd, uvd, uvh],
      corners: cornersFor(0, 1, hw, hh, hd),
    },
    { normal: [0, 0, -1], uv: uvZMinus, corners: cornersFor(2, -1, hw, hh, hd) },
  ];
}

/** 皮肤 UV 原点（64×64，Mojang 公开布局）。 */
const SKIN_UV = {
  head: [0, 0],
  hat: [32, 0],
  body: [16, 16],
  jacket: [16, 32],
  rightArm: [40, 16],
  rightSleeve: [40, 32],
  leftArm: [32, 48],
  leftSleeve: [48, 48],
  rightLeg: [0, 16],
  rightPants: [0, 32],
  leftLeg: [16, 48],
  leftPants: [0, 48],
} as const;

/** 第二层膨胀量：头 +0.5，其余 +0.25（Mojang 规范）。 */
const INFLATE_HEAD = 0.5;
const INFLATE_LIMB = 0.25;

/** 组装玩家模型。`slim` = 纤细手臂（宽 3 而不是 4）。
 *  坐标：原点在双脚之间的地面中心，y 向上，z 指向模型正面。
 *
 *  `legacy` = 64×32 旧格式：没有独立的左臂/左腿 UV，左右两侧共用右侧贴图（显示时镜像），
 *  第二层也只有头部有。用专有的 64×64 左臂 UV 在 32 高的图上会取到越界/空白。 */
function buildPlayer(slim: boolean, legacy: boolean): VoxelBox[] {
  const armW = slim ? 3 : 4;
  // 深度恒为 4：纤细模型只把手臂**宽度**收窄，厚度不变。
  // （曾错写成 armD = armW，导致 slim 的 UV 展开只有 12 宽而贴图是 14 —— 整条手臂错位两列，
  //  表现就是肩膀边缘缺一个像素、能看到背景。）
  const armD = 4;
  const armX = slim ? 5.5 : 6;
  const rightArmUv = SKIN_UV.rightArm;
  const rightLegUv = SKIN_UV.rightLeg;
  const leftArmUv = legacy ? rightArmUv : SKIN_UV.leftArm;
  const leftLegUv = legacy ? rightLegUv : SKIN_UV.leftLeg;
  const mk = (
    cx: number,
    cy: number,
    w: number,
    h: number,
    d: number,
    uv: readonly [number, number],
    inflate: number,
    mirror = false,
  ): VoxelBox => ({ cx, cy, cz: 0, w, h, d, u: uv[0], v: uv[1], inflate, mirror });
  return [
    mk(0, 28, 8, 8, 8, SKIN_UV.head, 0),
    mk(0, 28, 8, 8, 8, SKIN_UV.hat, INFLATE_HEAD),
    mk(0, 18, 8, 12, 4, SKIN_UV.body, 0),
    mk(0, 18, 8, 12, 4, SKIN_UV.jacket, INFLATE_LIMB),
    mk(-armX, 18, armW, 12, armD, rightArmUv, 0),
    mk(-armX, 18, armW, 12, armD, SKIN_UV.rightSleeve, INFLATE_LIMB),
    mk(armX, 18, armW, 12, armD, leftArmUv, 0, legacy),
    mk(
      armX,
      18,
      armW,
      12,
      armD,
      legacy ? SKIN_UV.rightSleeve : SKIN_UV.leftSleeve,
      INFLATE_LIMB,
      legacy,
    ),
    mk(-2, 6, 4, 12, 4, rightLegUv, 0),
    mk(-2, 6, 4, 12, 4, SKIN_UV.rightPants, INFLATE_LIMB),
    mk(2, 6, 4, 12, 4, leftLegUv, 0, legacy),
    mk(2, 6, 4, 12, 4, legacy ? SKIN_UV.rightPants : SKIN_UV.leftPants, INFLATE_LIMB, legacy),
  ];
}

/** 披风：10×16×1 的板，挂在身体背后（z 负向），从肩部垂到小腿。
 *  贴图是**独立的** 64×32 披风纹理，立方体 UV 展开与皮肤同规则（原点 0,0）。
 *
 *  `swing` 给一个固定的下垂外张（绕肩部支点，底端向后摆开）：
 *  完全贴背的话，默认的 3/4 正面视角会把披风整个挡在身后，用户选了披风却看不到。
 *  外张后底端摆出约 2 体素，正面视角下能从身体右侧露出一角，背面视角照常看全。 */
function buildCape(): VoxelBox[] {
  return [
    {
      cx: 0,
      cy: 16,
      cz: -2.5,
      w: 10,
      h: 16,
      d: 1,
      u: 0,
      v: 0,
      inflate: 0,
      cape: true,
      flipZ: true, // 披风朝外的面是 -z，图案（north 区）要显示在这一面
      swing: CAPE_DRAPE_DEG,
      pivot: [0, 24, -2.5], // 肩部：披风从这里垂下
    },
  ];
}

/** 鞘翅静止姿态：绕 X 前倾 15°（官方 rotation.x = 0.2618 rad）。 */
const ELYTRA_TILT_DEG = 15;
/** 鞘翅静止姿态：绕 Z 张开 15°（官方 rotation.z = 0.2618 rad）。
 *
 *  静止态就是**收拢**的——两片翅膀近乎垂直贴着后背叠在一起、合起来呈壳状，
 *  所以官方才要额外给一点点 Y 轴旋转（skinview3d 的 `rotation.y = 0.01`）来避免两片共面闪烁。
 *  真正展开成水平翅膀是滑翔时才有的形态，本项目不需要（衣柜预览的是站立态）。
 *
 *  注意 15° 这个值不能随便放大：翅膀贴图的图案是按**竖直朝向**画的，
 *  张开角一大会把图案整体拧过去，看起来就像"把披风横向拉伸成翅膀"。 */
const ELYTRA_REST_SPREAD_DEG = 15;

/** 鞘翅（按官方模型 1:1）。
 *
 *  几何 `12×22×4` = MC `ElytraModel` 的基础盒 `10×20×2` 加 `CubeDeformation(1)` 膨胀；
 *  UV 按**基础盒** `10×20×2` 展开，即 `texOffs(22,0)` ⇒ 取样区 `(22,0)-(46,22)`。
 *  这一点有实物可查：官方 `elytra.png`（64×32）的图案正落在 x=22..45，其余全透明。
 *
 *  贴图用**披风纹理**——这正是 MC 的行为，但关键在于**取样哪一块**：
 *  Mojang 的披风纹理是双用途的，左半 `(0,0)-(22,17)` 是披风展开（披风模型用），
 *  右半 `(22,0)-(46,22)` 是**同一布局的鞘翅图案**（另外画的，不是把披风图案切两半）。
 *  实测迁移者披风纹理：右半与该 UV 区逐像素重合，但配色是迁移者自己的红黑，
 *  与官方灰色 elytra.png 图案轮廓一致、颜色不同。
 *
 *  早先的实现把披风展开区 `(0,0)-(22,17)` 拉伸到翅膀上，于是两片翅膀拼起来能看出原披风
 *  —— 那不是 MC 的行为。改成取 `(22,0)` 后，翅膀显示的是纹理里真正的鞘翅图案。
 *
 *  另外翅膀贴图只画在**朝外那一面**（另一面全透明），所以必须 noCull，
 *  否则从正面看背面被剔除、正面又全透明，翅膀会整个消失。 */
function buildElytra(): VoxelBox[] {
  const mk = (sign: 1 | -1): VoxelBox => ({
    cx: 0,
    // 位置换算自 skinview3d 的 ElytraObject（其模型脚底在 y=-16，本渲染器在 y=0）：
    //   group (0,8,-2) + mesh 偏移 (∓5,-10,-1) ⇒ 盒子中心 (0,-2,-3) ⇒ 本坐标系 (0,14,-3)
    //   旋转支点＝group 原点 (0,8,-2) ⇒ 本坐标系 (±5,24,-2)（y=24 正是本模型的肩线）
    cy: 14,
    cz: -3,
    w: 12,
    h: 22,
    d: 4,
    // 官方 texOffs(22,0) + 基础盒 10×20×2
    u: 22,
    v: 0,
    uvW: 10,
    uvH: 20,
    uvD: 2,
    inflate: 0,
    cape: true,
    noCull: true,
    // 静止姿态＝官方默认：rotation.x = 0.2618 rad(15°)、rotation.z = 0.2618 rad(15°)。
    // 翅膀贴近竖直、两片在后背叠成壳状（与 skinview3d 的 ElytraObject.resetJoints 一致）。
    swing: ELYTRA_TILT_DEG,
    swingZ: sign * ELYTRA_REST_SPREAD_DEG,
    // 旋转中心在肩部（对应 skinview3d 的 leftWing.position = (5,0,0)，即 group 原点）
    pivot: [sign * 5, 24, -2],
    mirror: sign < 0,
  });
  return [mk(-1), mk(1)];
}

/** 披风下垂外张角度（度）：底端向后摆开，使默认 3/4 视角能看到它。
 *  静止贴背的话正面完全看不到披风；12° 时底端摆出约 3.3 体素，
 *  正面视角能从身体右侧露出一条明显边缘，背面视角仍完整。 */
const CAPE_DRAPE_DEG = 12;

/** 披风动效：对齐 skinview3d 的官方实现——
 *  `cape.rotation.x = sin(t/1.5) * 0.06 + 0.188`（行走）、`sin(t) * 0.01 + 0.188`（待机）。
 *  基础外张用本渲染器的 CAPE_DRAPE_DEG（约合 0.21 rad，与官方的 0.188 接近），
 *  摆动幅度 ±3.4°(行走) / ±0.6°(待机)。 */
function applyCapeMotion(capes: VoxelBox[], anim: Animation, phase: number): void {
  if (anim === "none") return;
  const wave =
    anim === "walk" ? Math.sin((phase * 8) / 1.5) * 3.4 : Math.sin(phase * 2) * 0.6;
  for (const c of capes) c.swing = CAPE_DRAPE_DEG + wave;
}

/** 只取头部（头像用）：头 + 帽子层，中心在原点。 */
function buildHead(): VoxelBox[] {
  const mk = (
    w: number,
    h: number,
    d: number,
    uv: readonly [number, number],
    inflate: number,
  ): VoxelBox => ({ cx: 0, cy: 0, cz: 0, w, h, d, u: uv[0], v: uv[1], inflate });
  return [mk(8, 8, 8, SKIN_UV.head, 0), mk(8, 8, 8, SKIN_UV.hat, INFLATE_HEAD)];
}

/** 六个朝向的固定明暗系数（模拟 Minecraft 的逐面着色）。
 *  **正面与顶面为 1.0（不做压暗）**：正脸头像是主用途，压暗会让整张脸比原皮肤暗一档、显灰；
 *  侧面/背面/底面仍保留差异，3/4 视角的立体感不受影响。 */
function shadeFor(normal: readonly [number, number, number]): number {
  if (normal[1] > 0) return 1.0; // 顶
  if (normal[1] < 0) return 0.55; // 底
  if (normal[2] > 0) return 1.0; // 正面
  if (normal[2] < 0) return 0.85; // 背面
  return 0.66; // 左 / 右
}

/** 正交投影：先绕 Y（yaw）再绕 X（pitch）。返回屏幕坐标（y 向下）与深度。 */
function project(
  x: number,
  y: number,
  z: number,
  cosY: number,
  sinY: number,
  cosP: number,
  sinP: number,
): [number, number, number] {
  const x1 = x * cosY - z * sinY;
  const z1 = x * sinY + z * cosY;
  const y2 = y * cosP - z1 * sinP;
  const z2 = y * sinP + z1 * cosP;
  return [x1, -y2, z2];
}

/** 已投影的一个待绘制面。 */
interface PaintFace {
  /** 屏幕四角（扁平 [x0,y0,x1,y1,x2,y2,x3,y3]，顺序＝左下、右下、右上、左上） */
  pts: number[];
  uv: UvRect;
  shade: number;
  depth: number;
  /** 贴图水平镜像（64×32 旧格式左侧肢体） */
  mirror: boolean;
  /** 0 = 第一层（本体），1 = 第二层（帽子/外套/袖腿） */
  layer: 0 | 1;
  /** 披风/鞘翅面（用的是独立纹理，不属于皮肤贴图） */
  cape: boolean;
  /** 该面使用的贴图；未给则用主皮肤贴图（披风面用披风贴图） */
  tex?: CanvasImageSource;
}

/** 一次绘制留下的**可拾取面**（按绘制顺序：远 → 近）。
 *  供「3D 直绘」把屏幕点击反查成贴图像素：面在正交投影下是平行四边形，
 *  `m` 就是它「贴图像素 → 画布设备像素」的仿射矩阵，求逆即得反查。 */
export interface PickFace {
  /** 贴图取样矩形 [u, v, w, h]。 */
  uv: UvRect;
  /** 仿射矩阵 [a, b, c, d, e, f]，同 ctx.transform。 */
  m: readonly [number, number, number, number, number, number];
  /** 0 = 第一层（本体），1 = 第二层。 */
  layer: 0 | 1;
  /** 披风/鞘翅面不可拾取（贴图坐标落在披风纹理上，不是皮肤像素）。 */
  cape: boolean;
}

/** 一次绘制的构图参数。 */
interface PaintOpts {
  /** 视角（度）。 */
  yaw: number;
  pitch: number;
  /** 模型在画布中的占比。 */
  fill: number;
  /** `bbox` = 按可见包围盒贴合填满（头像，永远居中填满）；
   *  `fixed` = 按名义尺寸 (nominalW×nominalH 体素) 固定构图（全身，姿势变化时不会忽大忽小）。 */
  fit?: "bbox" | "fixed";
  nominalW?: number;
  nominalH?: number;
  /** 披风贴图（独立 64×32 纹理）。 */
  capeTex?: CanvasImageSource;
  /** 传入则回填本次绘制的可拾取面（清空后按绘制顺序追加），供 3D 直绘反查。 */
  hits?: PickFace[];
}

/** 把模型正交等距画到 canvas 上。 */
function paint(
  canvas: HTMLCanvasElement,
  skin: CanvasImageSource,
  boxes: VoxelBox[],
  opts: PaintOpts,
): boolean {
  const ctx = canvas.getContext("2d");
  if (!ctx) return false;

  const fit = opts.fit ?? "bbox";
  const nominalW = opts.nominalW ?? 20;
  const nominalH = opts.nominalH ?? 34;
  const hits = opts.hits;
  if (hits) hits.length = 0;
  const yaw = (opts.yaw * Math.PI) / 180;
  const pitch = (opts.pitch * Math.PI) / 180;
  const cosY = Math.cos(yaw);
  const sinY = Math.sin(yaw);
  const cosP = Math.cos(pitch);
  const sinP = Math.sin(pitch);

  const faces: PaintFace[] = [];
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (const b of boxes) {
    for (const f of facesOf(b)) {
      // 背面剔除：本投影约定 +z 朝向观察者，法线转到视空间后深度分量为正即面向观察者
      // （noCull 的盒子两面都画，靠画家算法定序；透明面不覆盖已画好的像素，天然安全）
      if (!b.noCull) {
        const nz1 = f.normal[0] * sinY + f.normal[2] * cosY;
        const nDepth = f.normal[1] * sinP + nz1 * cosP;
        if (nDepth <= 1e-4) continue;
      }

      const pts: number[] = [];
      let depthSum = 0;
      // 四肢/头部姿态：顶点绕支点依次做 X（前后摆）→ Y（转头）→ Z（外张）旋转
      const swing = ((b.swing ?? 0) * Math.PI) / 180;
      const cosS = Math.cos(swing);
      const sinS = Math.sin(swing);
      const swingY = ((b.swingY ?? 0) * Math.PI) / 180;
      const cosYy = Math.cos(swingY);
      const sinYy = Math.sin(swingY);
      const swingZ = ((b.swingZ ?? 0) * Math.PI) / 180;
      const cosZ = Math.cos(swingZ);
      const sinZ = Math.sin(swingZ);
      const px0 = b.pivot ? b.pivot[0] : b.cx;
      const py0 = b.pivot ? b.pivot[1] : b.cy;
      const pz0 = b.pivot ? b.pivot[2] : b.cz;
      for (const c of f.corners) {
        let wx = b.cx + c[0];
        let wy = b.cy + c[1];
        let wz = b.cz + c[2];
        if (swing !== 0) {
          const ry = wy - py0;
          const rz = wz - pz0;
          wy = py0 + ry * cosS - rz * sinS;
          wz = pz0 + ry * sinS + rz * cosS;
        }
        if (swingY !== 0) {
          const rx = wx - px0;
          const rz = wz - pz0;
          wx = px0 + rx * cosYy + rz * sinYy;
          wz = pz0 - rx * sinYy + rz * cosYy;
        }
        if (swingZ !== 0) {
          const rx = wx - px0;
          const ry = wy - py0;
          wx = px0 + rx * cosZ - ry * sinZ;
          wy = py0 + rx * sinZ + ry * cosZ;
        }
        const [sx, sy, depth] = project(wx, wy, wz, cosY, sinY, cosP, sinP);
        pts.push(sx, sy);
        depthSum += depth;
        if (sx < minX) minX = sx;
        if (sx > maxX) maxX = sx;
        if (sy < minY) minY = sy;
        if (sy > maxY) maxY = sy;
      }
      faces.push({
        pts,
        uv: f.uv,
        shade: shadeFor(f.normal),
        depth: depthSum / f.corners.length,
        mirror: b.mirror === true,
        layer: b.inflate > 0 ? 1 : 0,
        cape: b.cape === true,
        tex: b.cape ? opts.capeTex : undefined,
      });
    }
  }
  if (faces.length === 0) return false;

  // 贴合裁切（头）或固定构图（全身）：把投影坐标映射到画布并居中
  let scale: number;
  let offX: number;
  let offY: number;
  if (fit === "fixed") {
    // 固定构图：以模型名义尺寸（体素）为准，模型的上下中心始终落在画布中心
    scale = Math.min(canvas.width / nominalW, canvas.height / nominalH) * opts.fill;
    const centerX = (minX + maxX) / 2;
    const centerY = (minY + maxY) / 2;
    offX = canvas.width / 2 - centerX * scale;
    offY = canvas.height / 2 - centerY * scale;
  } else {
    const spanX = maxX - minX || 1;
    const spanY = maxY - minY || 1;
    scale = Math.min(canvas.width / spanX, canvas.height / spanY) * opts.fill;
    offX = (canvas.width - spanX * scale) / 2 - minX * scale;
    offY = (canvas.height - spanY * scale) / 2 - minY * scale;
  }

  faces.sort((a, b) => a.depth - b.depth); // 画家算法：depth 小 = 远，先画

  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.imageSmoothingEnabled = false;

  for (const f of faces) {
    const [x0, y0, x1, y1, x2, y2, x3, y3] = f.pts;
    if (x0 === undefined || y0 === undefined || x1 === undefined || y1 === undefined ||
        x2 === undefined || y2 === undefined || x3 === undefined || y3 === undefined) continue;
    const sxp = [x0 * scale + offX, x1 * scale + offX, x2 * scale + offX, x3 * scale + offX];
    const syp = [y0 * scale + offY, y1 * scale + offY, y2 * scale + offY, y3 * scale + offY];
    const [u, v, uw, uh] = f.uv;

    // 正交投影下四角仍是平行四边形：用两个基向量（左下→右下 = +u，左下→左上 = -v）做仿射贴图。
    // 四角顺序是「左下、右下、右上、左上」，故 p0 对应贴图左下角 (u, v+uh)。
    // 镜像时 u 方向取反，p0 改对应贴图右下角 (u+uw, v+uh)。
    const uSign = f.mirror ? -1 : 1;
    const uBase = f.mirror ? u + uw : u;
    const m11 = (sxp[1]! - sxp[0]!) / (uw * uSign);
    const m12 = (syp[1]! - syp[0]!) / (uw * uSign);
    const m21 = (sxp[3]! - sxp[0]!) / -uh;
    const m22 = (syp[3]! - syp[0]!) / -uh;
    const tx = sxp[0]! - m11 * uBase - m21 * (v + uh);
    const ty = syp[0]! - m12 * uBase - m22 * (v + uh);

    // 逐面回填拾取信息：与绘制同源（同一组矩阵），直绘反查不会与画面错位
    if (hits) {
      hits.push({ uv: f.uv, m: [m11, m12, m21, m22, tx, ty], layer: f.layer, cape: f.cape });
    }

    ctx.save();
    ctx.beginPath();
    ctx.moveTo(sxp[0]!, syp[0]!);
    ctx.lineTo(sxp[1]!, syp[1]!);
    ctx.lineTo(sxp[2]!, syp[2]!);
    ctx.lineTo(sxp[3]!, syp[3]!);
    ctx.closePath();
    ctx.clip();
    ctx.transform(m11, m12, m21, m22, tx, ty);
    // 明暗绑在**这一次 drawImage** 上：brightness 只作用于本面画出的像素。
    // （不能用「画完再 source-atop 涂一层」——那会连带把下面别面画好的像素一起压暗，
    //   典型症状是透明帽子层与头重叠处被暗化两次，头像整体发灰。）
    ctx.filter = f.shade < 0.999 ? `brightness(${f.shade})` : "none";
    ctx.drawImage(f.tex ?? skin, 0, 0);
    ctx.restore();
  }
  return true;
}

/** 已解码皮肤图缓存（dataURL → Image），避免同一皮肤反复解码。 */
const skinImageCache = new Map<string, Promise<HTMLImageElement>>();

/** 解码皮肤 dataURL（失败 reject）。 */
export function loadSkinImage(dataUrl: string): Promise<HTMLImageElement> {
  const hit = skinImageCache.get(dataUrl);
  if (hit) return hit;
  const p = new Promise<HTMLImageElement>((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("皮肤解码失败"));
    img.src = dataUrl;
  });
  skinImageCache.set(dataUrl, p);
  return p;
}

/** dataURL 前缀归一化。 */
function toDataUrl(b64: string): string {
  return b64.startsWith("data:") ? b64 : `data:image/png;base64,${b64}`;
}

/** 常驻预览可用的动作。 */
export type Animation = "none" | "idle" | "walk";

/** 相机默认视角（度）：3/4 正面 + 微俯。 */
export const DEFAULT_VIEW_YAW = 30;
export const DEFAULT_VIEW_PITCH = 15;

/** 全身构图的体素尺寸（宽 × 高）。 */
export const BODY_VOXEL_W = 20;
export const BODY_VOXEL_H = 34;

/** 按动画与相位给四肢设定摆角，参数对齐 skinview3d 官方实现（WalkingAnimation / IdleAnimation）：
 *
 *  - 行走：`t = progress * 8`，四肢绕 X 轴 ±0.5 rad (±28.6°)，臂与同侧腿反相；
 *          手臂另有绕 Z 轴的外张（基础 0.02π≈3.6° + cos 摆动 ±1.72°）。
 *  - 待机：`t = progress * 2`，**腿完全不动**，只有手臂的 Z 轴外张。
 *  - 披风：`sin(t/1.5) * 0.06 + 0.06π`（约基础 10.8° + 摆动 ±3.4°）。
 *
 *  符号：skinview3d 的 rotation.x 正值让肢体向前，本实现的 swing 正值向后，故取反；
 *  绕 Z 轴的外张方向两侧对称（官方左臂 +、右臂 −），与本实现的臂位（左 +x / 右 −x）一致。 */
function applyAnimation(boxes: VoxelBox[], anim: Animation, phase: number): void {
  if (anim === "none") return;
  const setX = (i: number, angle: number, pivotY: number) => {
    const b = boxes[i];
    if (!b) return;
    b.swing = angle;
    b.pivot = [b.cx, pivotY, 0];
  };
  const setZ = (i: number, angle: number, pivotY: number) => {
    const b = boxes[i];
    if (!b) return;
    b.swingZ = angle;
    b.pivot = [b.cx, pivotY, 0];
  };
  // 手臂 Z 轴外张（官方 idle / walk 共用）：基础 3.6°，叠加 cos 摆动 ±1.72°
  const armZ = (t: number) => 3.6 + Math.cos(t) * 1.72;
  const setArms = (t: number) => {
    const z = armZ(t);
    // 4/5 = 右臂（−x 侧，外张取负）；6/7 = 左臂（+x 侧，外张取正）
    setZ(4, -z, 24);
    setZ(5, -z, 24);
    setZ(6, z, 24);
    setZ(7, z, 24);
  };

  // 约定 box 顺序：4/5 右臂(+袖)，6/7 左臂(+袖)，8/9 右腿(+裤)，10/11 左腿(+裤)
  if (anim === "walk") {
    const t = phase * 8;
    const AMP = 28.6; // 0.5 rad
    const legL = -Math.sin(t) * AMP;
    const legR = -Math.sin(t + Math.PI) * AMP;
    setX(10, legL, 12);
    setX(11, legL, 12);
    setX(8, legR, 12);
    setX(9, legR, 12);
    // 臂绕 X 与同侧腿反相
    setX(6, -legL, 24);
    setX(7, -legL, 24);
    setX(4, -legR, 24);
    setX(5, -legR, 24);
    setArms(t);
    // 头部摆动（官方 headBobbing：rotation.y = sin(t/4)*0.2、rotation.x = sin(t/5)*0.1）
    // 头与帽子层（0/1）一起转，绕头自身中心。Y 轴符号与官方相反（本实现正向朝 +x 偏）。
    const headX = Math.sin(t / 5) * 5.73;
    const headY = -Math.sin(t / 4) * 11.46;
    for (const hi of [0, 1]) {
      const hb = boxes[hi];
      if (!hb) continue;
      hb.swing = headX;
      hb.swingY = headY;
      hb.pivot = [hb.cx, hb.cy, hb.cz];
    }
    return;
  }
  // 待机：腿不动，只有手臂外张
  setArms(phase * 2);
}

/** 把皮肤画到给定 canvas（衣柜实时预览 / 离屏取图共用）。
 *  `fill` 控制模型在画布中的占比（0.4 = 占画布高约 40%），由调用方按交互缩放传入。
 *  传入 `capeTex` 时挂上披风（独立的 64×32 披风纹理）；
 *  `elytra` 为真时改为渲染**展开的鞘翅形态**（同一张披风纹理，取样同一 UV）。 */
export function drawFullCanvas(
  canvas: HTMLCanvasElement,
  skin: CanvasImageSource,
  opts: {
    yaw: number;
    pitch: number;
    slim: boolean;
    legacy?: boolean;
    animation?: Animation;
    phase?: number;
    fill?: number;
    capeTex?: CanvasImageSource;
    elytra?: boolean;
    /** 传入则回填本次绘制的可拾取面（供 3D 直绘反查贴图像素）。 */
    hits?: PickFace[];
  },
): boolean {
  const boxes = buildPlayer(opts.slim, opts.legacy ?? false);
  const anim = opts.animation ?? "none";
  const phase = opts.phase ?? 0;
  applyAnimation(boxes, anim, phase);
  // 披风/鞘翅追加在四肢之后：applyAnimation 按固定下标 [4..11] 取四肢，追加不影响；
  // 但它们自己的摆动要单独给（下标方案覆盖不到）
  if (opts.capeTex) {
    if (opts.elytra) {
      boxes.push(...buildElytra());
    } else {
      const capes = buildCape();
      applyCapeMotion(capes, anim, phase);
      boxes.push(...capes);
    }
  }
  return paint(canvas, skin, boxes, {
    yaw: opts.yaw,
    pitch: opts.pitch,
    fill: opts.fill ?? 0.4,
    fit: "fixed",
    nominalW: BODY_VOXEL_W,
    nominalH: BODY_VOXEL_H,
    capeTex: opts.capeTex,
    hits: opts.hits,
  });
}

/** 皮肤 base64 → 头部头像 dataURL（正方形，默认正脸平视）。 */
export async function renderHeadDataUrl(
  b64: string,
  px = 96,
  yawDeg = 0,
  pitchDeg = 0,
): Promise<string> {
  const img = await loadSkinImage(toDataUrl(b64));
  const canvas = document.createElement("canvas");
  canvas.width = px;
  canvas.height = px;
  if (!paint(canvas, img, buildHead(), { yaw: yawDeg, pitch: pitchDeg, fill: 0.98 })) return "";
  return canvas.toDataURL("image/png");
}

/** 皮肤 base64 → 全身 dataURL（按 20:34 体素构图）。`fill` = 模型在画布中的占比。 */
export async function renderFullDataUrl(
  b64: string,
  slim: boolean,
  ppv: number,
  yawDeg = DEFAULT_VIEW_YAW,
  pitchDeg = DEFAULT_VIEW_PITCH,
  fill = 0.4,
): Promise<string> {
  const img = await loadSkinImage(toDataUrl(b64));
  const canvas = document.createElement("canvas");
  canvas.width = Math.ceil(BODY_VOXEL_W * ppv);
  canvas.height = Math.ceil(BODY_VOXEL_H * ppv);
  if (!drawFullCanvas(canvas, img, { yaw: yawDeg, pitch: pitchDeg, slim, legacy: img.naturalHeight < 64, fill })) return "";
  return canvas.toDataURL("image/png");
}
