#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>
typedef struct InkContext InkContext;
extern uint64_t ink_literal(InkContext*,uint32_t,uint64_t,const char*,size_t);
extern uint64_t ink_op(InkContext*,const char*,const uint64_t*,size_t);
/* Frame tokens delimit invocation-local scratch. Only the returned handle and
 * pre-frame handles survive finish; tokens are private to literal emitted code. */
extern uint64_t ink_value_frame(InkContext*);
extern uint64_t ink_value_finish_frame(InkContext*,uint64_t,uint64_t);
extern uint64_t ink_item(InkContext*,uint64_t,size_t);
extern void ink_append(InkContext*,uint64_t,uint64_t);
extern size_t ink_length(InkContext*,uint64_t);
extern bool ink_bool(InkContext*,uint64_t);
extern uint32_t ink_u32(InkContext*,uint64_t);
extern bool ink_failed(InkContext*);
extern bool ink_tick(InkContext*);
extern uint64_t ink_error(InkContext*,const char*);
extern uint64_t ink_finish_action(InkContext*,uint64_t);
uint64_t ink_program_call(InkContext*,uint32_t,uint32_t,const uint64_t*,size_t);

typedef uint64_t (*InkParallelCallback)(InkContext*,uint32_t,uint64_t,const uint64_t*);
extern uint64_t ink_parallel(InkContext*,uint64_t,uint32_t,uint32_t,const uint64_t*,size_t,uint32_t);
