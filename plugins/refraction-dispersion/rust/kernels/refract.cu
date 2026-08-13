// Stage 3 passthrough kernel.
//
// Copies ARGB u8 pixels from input to output unchanged.
// Exists only to prove the CUDA pipeline (PTX load -> memcpy H2D ->
// kernel launch -> memcpy D2H) is wired up end-to-end before we commit
// to writing the real refraction kernel.
//
// Pixel layout: one uchar4 per pixel, same as AE's ARGB_8u world.

extern "C" __global__
void passthrough_kernel(const uchar4* __restrict__ src,
                        uchar4* __restrict__ dst,
                        int w,
                        int h)
{
    int x = blockIdx.x * blockDim.x + threadIdx.x;
    int y = blockIdx.y * blockDim.y + threadIdx.y;
    if (x >= w || y >= h) return;
    int i = y * w + x;
    dst[i] = src[i];
}
