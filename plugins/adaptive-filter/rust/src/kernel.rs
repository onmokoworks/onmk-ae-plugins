/// OpenCL kernel source for GPU rendering.
/// Pixel format: float4 BGRA (x=B, y=G, z=R, w=A), range 0.0-1.0.

#[allow(dead_code)]
pub const OPENCL_KERNEL_SOURCE: &str = r#"

inline float4 readClamped(__global const float4* src, int x, int y, int w, int h, int pitch)
{
    x = clamp(x, 0, w - 1);
    y = clamp(y, 0, h - 1);
    return src[y * pitch + x];
}

inline float4 makePixel(float b, float g, float r, float a)
{
    return (float4)(b, g, r, a);
}

__kernel void MedianFilterKernel(
    __global const float4* src, __global float4* dst,
    int srcPitch, int dstPitch, int width, int height, int radius)
{
    int x = get_global_id(0);
    int y = get_global_id(1);
    if (x >= width || y >= height) return;

    int hist[256];
    int count = 0;
    float outCh[3];
    float4 center = readClamped(src, x, y, width, height, srcPitch);

    for (int ch = 0; ch < 3; ch++) {
        for (int i = 0; i < 256; i++) hist[i] = 0;
        count = 0;
        for (int ky = -radius; ky <= radius; ky++) {
            for (int kx = -radius; kx <= radius; kx++) {
                float4 p = readClamped(src, x + kx, y + ky, width, height, srcPitch);
                float val = (ch == 0) ? p.x : (ch == 1) ? p.y : p.z;
                int bin = clamp((int)(val * 255.0f + 0.5f), 0, 255);
                hist[bin]++;
                count++;
            }
        }
        int median = count / 2;
        int cum = 0;
        for (int i = 0; i < 256; i++) {
            cum += hist[i];
            if (cum > median) { outCh[ch] = (float)i / 255.0f; break; }
        }
    }
    dst[y * dstPitch + x] = makePixel(outCh[0], outCh[1], outCh[2], center.w);
}

__kernel void WeightedMedianFilterKernel(
    __global const float4* src, __global float4* dst,
    int srcPitch, int dstPitch, int width, int height, int radius, float edgeParam)
{
    int x = get_global_id(0);
    int y = get_global_id(1);
    if (x >= width || y >= height) return;

    float sigma = max(0.5f, (float)radius * (edgeParam / 100.0f * 1.5f + 0.3f));
    float invSigma2 = 1.0f / (2.0f * sigma * sigma);
    float histW[256];
    float outCh[3];
    float4 center = readClamped(src, x, y, width, height, srcPitch);

    for (int ch = 0; ch < 3; ch++) {
        for (int i = 0; i < 256; i++) histW[i] = 0.0f;
        float totalW = 0.0f;
        for (int ky = -radius; ky <= radius; ky++) {
            for (int kx = -radius; kx <= radius; kx++) {
                float wt = exp(-(float)(kx * kx + ky * ky) * invSigma2);
                float4 p = readClamped(src, x + kx, y + ky, width, height, srcPitch);
                float val = (ch == 0) ? p.x : (ch == 1) ? p.y : p.z;
                int bin = clamp((int)(val * 255.0f + 0.5f), 0, 255);
                histW[bin] += wt;
                totalW += wt;
            }
        }
        float half = totalW * 0.5f;
        float cum = 0.0f;
        for (int i = 0; i < 256; i++) {
            cum += histW[i];
            if (cum >= half) { outCh[ch] = (float)i / 255.0f; break; }
        }
    }
    dst[y * dstPitch + x] = makePixel(outCh[0], outCh[1], outCh[2], center.w);
}

__kernel void KuwaharaFilterKernel(
    __global const float4* src, __global float4* dst,
    int srcPitch, int dstPitch, int width, int height, int radius)
{
    int x = get_global_id(0);
    int y = get_global_id(1);
    if (x >= width || y >= height) return;

    float4 center = readClamped(src, x, y, width, height, srcPitch);
    float bestVar = 1e30f, bestR = 0, bestG = 0, bestB = 0;
    int qx0[4] = { -radius, 0, -radius, 0 };
    int qx1[4] = { 0, radius, 0, radius };
    int qy0[4] = { -radius, -radius, 0, 0 };
    int qy1[4] = { 0, 0, radius, radius };

    for (int q = 0; q < 4; q++) {
        float sR=0,sG=0,sB=0,sR2=0,sG2=0,sB2=0;
        int cnt=0;
        for (int ky = qy0[q]; ky <= qy1[q]; ky++) {
            for (int kx = qx0[q]; kx <= qx1[q]; kx++) {
                float4 p = readClamped(src, x+kx, y+ky, width, height, srcPitch);
                sR+=p.z; sG+=p.y; sB+=p.x;
                sR2+=p.z*p.z; sG2+=p.y*p.y; sB2+=p.x*p.x;
                cnt++;
            }
        }
        float inv = 1.0f/(float)cnt;
        float mR=sR*inv, mG=sG*inv, mB=sB*inv;
        float v = (sR2*inv-mR*mR)+(sG2*inv-mG*mG)+(sB2*inv-mB*mB);
        if (v < bestVar) { bestVar=v; bestR=mR; bestG=mG; bestB=mB; }
    }
    dst[y*dstPitch+x] = makePixel(clamp(bestB,0.0f,1.0f), clamp(bestG,0.0f,1.0f), clamp(bestR,0.0f,1.0f), center.w);
}

__kernel void GenKuwaharaFilterKernel(
    __global const float4* src, __global float4* dst,
    int srcPitch, int dstPitch, int width, int height, int radius, float edgeParam)
{
    int x = get_global_id(0);
    int y = get_global_id(1);
    if (x >= width || y >= height) return;

    const float PI = 3.14159265358979f;
    const float sectorAngle = 2.0f * PI / 8.0f;
    float sharpness = 1.0f + edgeParam * 0.07f;
    float sR[8]={0,0,0,0,0,0,0,0}, sG[8]={0,0,0,0,0,0,0,0}, sB[8]={0,0,0,0,0,0,0,0};
    float sR2[8]={0,0,0,0,0,0,0,0}, sG2[8]={0,0,0,0,0,0,0,0}, sB2[8]={0,0,0,0,0,0,0,0};
    float sW[8]={0,0,0,0,0,0,0,0};
    float4 center = readClamped(src, x, y, width, height, srcPitch);
    float rf = (float)radius;

    for (int ky = -radius; ky <= radius; ky++) {
        for (int kx = -radius; kx <= radius; kx++) {
            float dist = sqrt((float)(kx*kx+ky*ky));
            if (dist > rf+0.5f) continue;
            float angle = atan2((float)ky,(float)kx)+PI;
            int sec = ((int)(angle/sectorAngle))%8;
            float sw = exp(-dist*dist/(2.0f*rf*rf*0.25f));
            float4 p = readClamped(src, x+kx, y+ky, width, height, srcPitch);
            sR[sec]+=p.z*sw; sG[sec]+=p.y*sw; sB[sec]+=p.x*sw;
            sR2[sec]+=p.z*p.z*sw; sG2[sec]+=p.y*p.y*sw; sB2[sec]+=p.x*p.x*sw;
            sW[sec]+=sw;
        }
    }
    float tR=0,tG=0,tB=0,tW=0;
    for (int s=0; s<8; s++) {
        if (sW[s]<1e-6f) continue;
        float inv=1.0f/sW[s];
        float mR=sR[s]*inv, mG=sG[s]*inv, mB=sB[s]*inv;
        float v=max(0.0f,(sR2[s]*inv-mR*mR)+(sG2[s]*inv-mG*mG)+(sB2[s]*inv-mB*mB));
        float wt=exp(-v*sharpness/(1.0f*0.01f));
        tR+=mR*wt; tG+=mG*wt; tB+=mB*wt; tW+=wt;
    }
    if (tW<1e-6f) tW=1.0f;
    dst[y*dstPitch+x] = makePixel(clamp(tB/tW,0.0f,1.0f), clamp(tG/tW,0.0f,1.0f), clamp(tR/tW,0.0f,1.0f), center.w);
}

__kernel void BilateralFilterKernel(
    __global const float4* src, __global float4* dst,
    int srcPitch, int dstPitch, int width, int height, int radius, float edgeParam)
{
    int x = get_global_id(0);
    int y = get_global_id(1);
    if (x >= width || y >= height) return;

    float sigmaS = max(1.0f, (float)radius*0.5f);
    float sigmaR = (5.0f+edgeParam*2.5f)/255.0f;
    float invSS2 = 1.0f/(2.0f*sigmaS*sigmaS);
    float invSR2 = 1.0f/(2.0f*sigmaR*sigmaR);
    float4 c = readClamped(src, x, y, width, height, srcPitch);
    float sR=0,sG=0,sB=0,sW=0;

    for (int ky=-radius; ky<=radius; ky++) {
        for (int kx=-radius; kx<=radius; kx++) {
            float4 p = readClamped(src, x+kx, y+ky, width, height, srcPitch);
            float dR=p.z-c.z, dG=p.y-c.y, dB=p.x-c.x;
            float wt = exp(-(float)(kx*kx+ky*ky)*invSS2-(dR*dR+dG*dG+dB*dB)*invSR2);
            sR+=p.z*wt; sG+=p.y*wt; sB+=p.x*wt; sW+=wt;
        }
    }
    if (sW<1e-6f) sW=1.0f;
    dst[y*dstPitch+x] = makePixel(clamp(sB/sW,0.0f,1.0f), clamp(sG/sW,0.0f,1.0f), clamp(sR/sW,0.0f,1.0f), c.w);
}

__kernel void MixKernel(
    __global const float4* original, __global const float4* filtered, __global float4* dst,
    int origPitch, int filtPitch, int dstPitch, int width, int height, float mix)
{
    int x = get_global_id(0);
    int y = get_global_id(1);
    if (x >= width || y >= height) return;
    float4 o = original[y*origPitch+x];
    float4 f = filtered[y*filtPitch+x];
    float4 r;
    r.x = o.x*(1.0f-mix)+f.x*mix;
    r.y = o.y*(1.0f-mix)+f.y*mix;
    r.z = o.z*(1.0f-mix)+f.z*mix;
    r.w = o.w;
    dst[y*dstPitch+x] = r;
}

"#;
