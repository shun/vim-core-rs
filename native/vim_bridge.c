#include "vim_bridge.h"
#include "upstream_runtime.h"

#include <stdlib.h>
#include <stdio.h>

struct vim_bridge_state {
    upstream_runtime_session_t* runtime;
};

vim_bridge_state_t* vim_bridge_state_new(
    const char* initial_text,
    uintptr_t text_len
) {
    vim_bridge_state_t* state = (vim_bridge_state_t*)calloc(1U, sizeof(vim_bridge_state_t));
    if (state == NULL) {
        return NULL;
    }

    state->runtime = upstream_runtime_session_new(initial_text, text_len);
    if (state->runtime == NULL) {
        free(state);
        return NULL;
    }

    return state;
}

void vim_bridge_state_free(vim_bridge_state_t* state) {
    if (state == NULL) {
        return;
    }

    upstream_runtime_session_free(state->runtime);
    free(state);
}

vim_core_snapshot_t vim_bridge_snapshot(const vim_bridge_state_t* state) {
    return upstream_runtime_snapshot(state != NULL ? state->runtime : NULL);
}

vim_core_command_result_t vim_bridge_apply_normal_command(
    vim_bridge_state_t* state,
    const char* command,
    uintptr_t command_len
) {
    if (state == NULL) {
        return upstream_runtime_apply_normal_command(NULL, command, command_len);
    }

    return upstream_runtime_apply_normal_command(state->runtime, command, command_len);
}

vim_core_command_result_t vim_bridge_apply_ex_command(
    vim_bridge_state_t* state,
    const char* command,
    uintptr_t command_len
) {
    if (state == NULL) {
        return upstream_runtime_apply_ex_command(NULL, command, command_len);
    }

    return upstream_runtime_apply_ex_command(state->runtime, command, command_len);
}

vim_host_action_t vim_bridge_take_pending_host_action(vim_bridge_state_t* state) {
    return upstream_runtime_take_pending_host_action(state != NULL ? state->runtime : NULL);
}

vim_runtime_backend_identity_t vim_bridge_backend_identity(
    const vim_bridge_state_t* state
) {
    return upstream_runtime_backend_identity(state != NULL ? state->runtime : NULL);
}

char* vim_bridge_get_register(const vim_bridge_state_t* state, char regname) {
    if (state == NULL) {
        return NULL;
    }
    return upstream_runtime_get_register(state->runtime, regname);
}

void vim_bridge_set_register(vim_bridge_state_t* state, char regname, const char* text, uintptr_t text_len) {
    if (state == NULL) {
        return;
    }
    upstream_runtime_set_register(state->runtime, regname, text, text_len);
}

void vim_bridge_free_string(char* ptr) {
    free(ptr);
}

vim_core_option_get_result_t vim_bridge_get_option(
    const vim_bridge_state_t* state,
    const char* name,
    vim_core_option_scope_t scope
) {
    printf("[DEBUG] vim_bridge_get_option: state=%p name=%s scope=%d\n",
           (const void*)state,
           name != NULL ? name : "(null)",
           (int)scope);
    if (state == NULL) {
        return upstream_runtime_get_option(NULL, name, scope);
    }

    return upstream_runtime_get_option(state->runtime, name, scope);
}

vim_core_option_set_result_t vim_bridge_set_option_number(
    vim_bridge_state_t* state,
    const char* name,
    int64_t value,
    vim_core_option_scope_t scope
) {
    printf("[DEBUG] vim_bridge_set_option_number: state=%p name=%s value=%lld scope=%d\n",
           (void*)state,
           name != NULL ? name : "(null)",
           (long long)value,
           (int)scope);
    if (state == NULL) {
        return upstream_runtime_set_option_number(NULL, name, value, scope);
    }

    return upstream_runtime_set_option_number(state->runtime, name, value, scope);
}

vim_core_option_set_result_t vim_bridge_set_option_string(
    vim_bridge_state_t* state,
    const char* name,
    const char* value,
    vim_core_option_scope_t scope
) {
    printf("[DEBUG] vim_bridge_set_option_string: state=%p name=%s value=%s scope=%d\n",
           (void*)state,
           name != NULL ? name : "(null)",
           value != NULL ? value : "(null)",
           (int)scope);
    if (state == NULL) {
        return upstream_runtime_set_option_string(NULL, name, value, scope);
    }

    return upstream_runtime_set_option_string(state->runtime, name, value, scope);
}

vim_core_status_t vim_bridge_apply_buffer_commit(
    vim_bridge_state_t* state,
    const vim_core_buffer_commit_t* commit
) {
    if (state == NULL || commit == NULL) return VIM_CORE_STATUS_SESSION_ERROR;
    return upstream_runtime_apply_buffer_commit(state->runtime, commit);
}

void vim_bridge_set_screen_size(vim_bridge_state_t* state, int rows, int cols) {
    if (state == NULL) return;
    upstream_runtime_set_screen_size(state->runtime, rows, cols);
}

vim_core_status_t vim_bridge_switch_to_buffer(vim_bridge_state_t* state, int buf_id) {
    if (state == NULL) return VIM_CORE_STATUS_SESSION_ERROR;
    return upstream_runtime_switch_to_buffer(state->runtime, buf_id);
}

vim_core_status_t vim_bridge_switch_to_window(vim_bridge_state_t* state, int win_id) {
    if (state == NULL) return VIM_CORE_STATUS_SESSION_ERROR;
    return upstream_runtime_switch_to_window(state->runtime, win_id);
}

char* vim_bridge_get_buffer_text(const vim_bridge_state_t* state, int buf_id) {
    if (state == NULL) return NULL;
    return upstream_runtime_get_buffer_text(state->runtime, buf_id);
}

vim_core_status_t vim_bridge_set_buffer_text(
    vim_bridge_state_t* state,
    int buf_id,
    const char* text,
    uintptr_t text_len
) {
    if (state == NULL) return VIM_CORE_STATUS_SESSION_ERROR;
    return upstream_runtime_set_buffer_text(state->runtime, buf_id, text, text_len);
}

vim_core_status_t vim_bridge_set_buffer_name(
    vim_bridge_state_t* state,
    int buf_id,
    const char* name,
    uintptr_t name_len
) {
    if (state == NULL) return VIM_CORE_STATUS_SESSION_ERROR;
    return upstream_runtime_set_buffer_name(state->runtime, buf_id, name, name_len);
}

vim_core_status_t vim_bridge_set_buffer_dirty(
    vim_bridge_state_t* state,
    int buf_id,
    bool dirty
) {
    if (state == NULL) return VIM_CORE_STATUS_SESSION_ERROR;
    return upstream_runtime_set_buffer_dirty(state->runtime, buf_id, dirty);
}

vim_core_pending_input_t vim_bridge_get_pending_input(const vim_bridge_state_t* state) {
    return upstream_runtime_get_pending_input(state != NULL ? state->runtime : NULL);
}

bool vim_bridge_get_mark(
    const vim_bridge_state_t* state,
    char mark_name,
    vim_core_mark_position_t* out_mark
) {
    return upstream_runtime_get_mark(state != NULL ? state->runtime : NULL, mark_name, out_mark);
}

vim_core_status_t vim_bridge_set_mark(
    vim_bridge_state_t* state,
    char mark_name,
    int buf_id,
    uintptr_t row,
    uintptr_t col
) {
    if (state == NULL) {
        return VIM_CORE_STATUS_SESSION_ERROR;
    }
    return upstream_runtime_set_mark(state->runtime, mark_name, buf_id, row, col);
}

vim_core_jumplist_t vim_bridge_get_jumplist(const vim_bridge_state_t* state) {
    return upstream_runtime_get_jumplist(state != NULL ? state->runtime : NULL);
}

void vim_bridge_free_jumplist(vim_core_jumplist_t jumplist) {
    upstream_runtime_free_jumplist(jumplist);
}

int vim_bridge_get_undo_tree(const vim_bridge_state_t* state, int buf_id, vim_core_undo_tree_t* out_tree) {
    if (state == NULL || state->runtime == NULL) return -1;
    return upstream_runtime_get_undo_tree(buf_id, out_tree);
}

void vim_bridge_free_undo_tree(vim_core_undo_tree_t tree) {
    upstream_runtime_free_undo_tree(tree);
}

int vim_bridge_undo_jump(vim_bridge_state_t* state, int buf_id, long seq) {
    if (state == NULL || state->runtime == NULL) return -1;
    return upstream_runtime_undo_jump(buf_id, seq);
}

int vim_bridge_get_line_syntax(const vim_bridge_state_t* state, int win_id, long lnum, int* out_ids, int max_cols) {
    if (state == NULL || state->runtime == NULL) return -1;
    return upstream_runtime_get_line_syntax(win_id, lnum, out_ids, max_cols);
}

const char* vim_bridge_get_syntax_name(const vim_bridge_state_t* state, int syn_id) {
    if (state == NULL || state->runtime == NULL) return NULL;
    return upstream_runtime_get_syntax_name(syn_id);
}

char* vim_bridge_eval_string(vim_bridge_state_t* state, const char* expr) {
    if (state == NULL || state->runtime == NULL || expr == NULL) return NULL;
    return upstream_runtime_eval_string(state->runtime, expr);
}

void vim_bridge_free_pum_info(vim_core_pum_info_t* pum) {
    if (pum == NULL) return;

    printf("[DEBUG] vim_bridge_free_pum_info: freeing pum=%p item_count=%zu\n",
           (void*)pum, pum->item_count);

    for (size_t i = 0; i < pum->item_count; i++) {
        /* 各候補の文字列フィールドを個別に解放（NULLチェック付き） */
        if (pum->items[i].word) free((void*)pum->items[i].word);
        if (pum->items[i].abbr) free((void*)pum->items[i].abbr);
        if (pum->items[i].menu) free((void*)pum->items[i].menu);
        if (pum->items[i].kind) free((void*)pum->items[i].kind);
        if (pum->items[i].info) free((void*)pum->items[i].info);
    }

    /* 候補配列を解放 */
    if (pum->items) free(pum->items);

    /* 構造体自体を解放 */
    free(pum);
}
