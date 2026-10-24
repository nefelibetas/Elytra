#ifndef ELYTRA_CUBIOMES_SHIM_H
#define ELYTRA_CUBIOMES_SHIM_H

#include <stdint.h>

#ifdef __cplusplus
extern "C"
{
#endif

typedef struct ElytraStructure
{
    int32_t type;
    int32_t x;
    int32_t z;
} ElytraStructure;

typedef struct ElytraChunk
{
    int32_t x;
    int32_t z;
} ElytraChunk;

void *elytra_gen_new(const char *mc_name, uint64_t seed);
void elytra_gen_free(void *gen);

int elytra_mc_from_name(const char *name);
const char *elytra_mc_name(int mc);

int elytra_biome_at(void *gen, int scale, int x, int y, int z);
int elytra_gen_biomes(void *gen, int scale, int x, int z, int sx, int sz, int y,
                      int *out);
const char *elytra_biome_name(int mc, int biome);

int elytra_max_y_quart(int mc);

int elytra_collect_structures(void *gen, int mc, uint64_t seed, int rx, int rz,
                              ElytraStructure *out, int cap);
int elytra_find_structures(void *gen, int mc, uint64_t seed, int type,
                           int center_x, int center_z, int radius_blocks,
                           ElytraStructure *out, int cap);
const char *elytra_structure_name(int type);

int elytra_collect_slime_chunks(uint64_t seed, int rx, int rz, ElytraChunk *out,
                                int cap);
int elytra_find_slime_chunks(uint64_t seed, int center_x, int center_z,
                             int radius_blocks, ElytraChunk *out, int cap);

int elytra_spawn(void *gen, int *x, int *z);

#ifdef __cplusplus
}
#endif

#endif
