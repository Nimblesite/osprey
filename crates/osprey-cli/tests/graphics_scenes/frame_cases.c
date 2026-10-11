static void reset(int stage) { failure = stage; memset(calls, 0, sizeof calls); }
static int failed_frame(int stage) {
  OspGfxContext ctx = {0};
  reset(stage);
  CHECK(osp_gfx_draw(&ctx) == 0);
  CHECK(ctx.closed == 1);
  int resets = calls[ALLOCATOR];
  CHECK(osp_gfx_draw(&ctx) == 0 && calls[ALLOCATOR] == resets);
  if (stage == PRESENT) { CHECK(calls[SIGNAL] == 1); }
  if (stage >= SIGNAL && stage <= REMOVED) {
    CHECK(osp_gfx_close(&ctx) == 0);
    CHECK(calls[DESTROY] == 0);
  }
  failure = NONE;
  CHECK(osp_gfx_close(&ctx) == 1 && calls[DESTROY] == 1);
  return 1;
}
static int successful_frame(void) {
  OspGfxContext ctx = {0};
  reset(NONE);
  CHECK(osp_gfx_draw(&ctx) == 1 && ctx.closed == 0);
  CHECK(calls[RECORD] == 1 && calls[EXECUTE] == 1 && calls[PRESENT] == 1);
  CHECK(calls[SIGNAL] == 1 && calls[EVENT] == 1 && calls[WAIT] == 1);
  CHECK(osp_gfx_close(&ctx) == 1 && calls[DESTROY] == 1);
  CHECK(osp_gfx_draw(NULL) == 0 && osp_gfx_close(NULL) == 0);
  return 1;
}
static int failed_close(int stage) {
  OspGfxContext ctx = {0};
  reset(stage);
  CHECK(osp_gfx_close(&ctx) == 0 && calls[DESTROY] == 0);
  CHECK(ctx.closed == 1);
  CHECK(osp_gfx_draw(&ctx) == 0 && calls[ALLOCATOR] == 0);
  failure = NONE;
  CHECK(osp_gfx_close(&ctx) == 1 && calls[DESTROY] == 1);
  return 1;
}
static int exhausted_fence(void) {
  OspGfxContext ctx = {0};
  ctx.fenceValue = UINT64_MAX;
  reset(NONE);
  CHECK(osp_gfx_close(&ctx) == 0 && ctx.closed == 1);
  CHECK(calls[SIGNAL] == 0 && calls[DESTROY] == 0);
  CHECK(ctx.fenceValue == UINT64_MAX);
  return 1;
}
int main(void) {
  int passed = successful_frame() & exhausted_fence();
  for (int stage = ALLOCATOR; stage <= REMOVED; stage++) { passed &= failed_frame(stage); }
  for (int stage = SIGNAL; stage <= REMOVED; stage++) { passed &= failed_close(stage); }
  return passed ? 0 : 1;
}
