// SPDX-License-Identifier: MIT
// Clang/MinGW bridge between Windows x64 and the embedded ELF System V ABI.
// The upstream MSVC build supplies these via MASM. Clang can generate the
// required argument shuffles and nonvolatile-register saves from ABI attributes.
#define SYSV __attribute__((sysv_abi))
void iree_elf_call_v_v(const void* address) {
  typedef void (SYSV *function)(void);
  ((function)address)();
}
void* iree_elf_call_p_i(const void* address, int a) {
  typedef void* (SYSV *function)(int);
  return ((function)address)(a);
}
void* iree_elf_call_p_ip(const void* address, int a, void* b) {
  typedef void* (SYSV *function)(int, void*);
  return ((function)address)(a, b);
}
int iree_elf_call_i_p(const void* address, void* a) {
  typedef int (SYSV *function)(void*);
  return ((function)address)(a);
}
int iree_elf_call_i_ppp(const void* address, void* a, void* b, void* c) {
  typedef int (SYSV *function)(void*, void*, void*);
  return ((function)address)(a, b, c);
}
void* iree_elf_call_p_ppp(const void* address, void* a, void* b, void* c) {
  typedef void* (SYSV *function)(void*, void*, void*);
  return ((function)address)(a, b, c);
}
SYSV int iree_elf_thunk_i_ppp(const void* address, void* a, void* b, void* c) {
  typedef int (*function)(void*, void*, void*);
  return ((function)address)(a, b, c);
}
