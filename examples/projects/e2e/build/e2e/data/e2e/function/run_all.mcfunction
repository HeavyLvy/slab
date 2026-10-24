scoreboard players add #g.loaded e2e 1
data modify storage e2e:globals names set value []
scoreboard players set #mathx__fib.v_n e2e 10
function e2e:mathx__fib
scoreboard players operation #g.r_fib e2e = #ret e2e
scoreboard players set #mathx__clamp.v_x e2e 100
scoreboard players set #mathx__clamp.v_lo e2e 0
scoreboard players set #mathx__clamp.v_hi e2e 50
function e2e:mathx__clamp
scoreboard players operation #g.r_clamp_hi e2e = #ret e2e
scoreboard players set #mathx__clamp.v_x e2e -5
scoreboard players set #mathx__clamp.v_lo e2e 0
scoreboard players set #mathx__clamp.v_hi e2e 50
function e2e:mathx__clamp
scoreboard players operation #g.r_clamp_lo e2e = #ret e2e
scoreboard players set #mathx__is_even.v_n e2e 10
function e2e:mathx__is_even
scoreboard players operation #run_all.t_0 e2e = #ret e2e
scoreboard players operation #run_all.t_0 e2e *= #c_10 e2e
scoreboard players set #mathx__is_even.v_n e2e 7
function e2e:mathx__is_even
scoreboard players operation #run_all.t_1 e2e = #ret e2e
scoreboard players operation #g.r_even e2e = #run_all.t_0 e2e
scoreboard players operation #g.r_even e2e += #run_all.t_1 e2e
scoreboard players set #run_all.v_i e2e 0
scoreboard players set #run_all.v_s e2e 0
scoreboard players set #run_all.v_brk e2e 0
scoreboard players set #run_all.v_cont e2e 0
function e2e:run_all/b0
scoreboard players operation #g.r_loop_break e2e = #run_all.v_s e2e
scoreboard players set #run_all.v_t e2e 0
scoreboard players set #run_all.v_k e2e 0
scoreboard players set #run_all.v_end e2e 10
scoreboard players set #run_all.v_brk_2 e2e 0
scoreboard players set #run_all.v_cont_2 e2e 0
function e2e:run_all/b7
scoreboard players operation #g.r_for_jump e2e = #run_all.v_t e2e
scoreboard players set #run_all.v_p e2e 1
scoreboard players set #run_all.v_k_2 e2e 1
scoreboard players set #run_all.v_end_2 e2e 5
function e2e:run_all/b12
scoreboard players operation #g.r_fact e2e = #run_all.v_p e2e
scoreboard players set #run_all.v_ls e2e 0
data modify storage e2e:vars run_all.v_iter set value [3,4,5]
function e2e:run_all/b13
scoreboard players set #run_all.v_cnt e2e 0
data modify storage e2e:vars run_all.v_iter_2 set value ["ann","bob","cy","dee"]
scoreboard players set #run_all.v_brk_3 e2e 0
function e2e:run_all/b14
scoreboard players operation #run_all.t_0 e2e = #run_all.v_ls e2e
scoreboard players operation #run_all.t_0 e2e *= #c_100 e2e
scoreboard players operation #g.r_for_list e2e = #run_all.t_0 e2e
scoreboard players operation #g.r_for_list e2e += #run_all.v_cnt e2e
scoreboard players set #run_all.v_n e2e 0
scoreboard players set #run_all.v__a e2e 0
scoreboard players set #run_all.v_end_3 e2e 4
function e2e:run_all/b17
scoreboard players operation #g.r_nested e2e = #run_all.v_n e2e
scoreboard players set #first_square_over.v_limit e2e 50
function e2e:first_square_over
scoreboard players operation #g.r_first_square e2e = #ret e2e
scoreboard players set #run_all.v_c e2e 0
function e2e:run_all/b21
scoreboard players operation #g.r_cond_call e2e = #run_all.v_c e2e
scoreboard players set #run_all.v_a e2e -7
scoreboard players set #run_all.v_b_2 e2e 2
scoreboard players operation #g.r_div e2e = #run_all.v_a e2e
scoreboard players operation #g.r_div e2e /= #run_all.v_b_2 e2e
scoreboard players operation #g.r_mod e2e = #run_all.v_a e2e
scoreboard players operation #g.r_mod e2e %= #run_all.v_b_2 e2e
scoreboard players operation #run_all.v_a e2e *= #c_3 e2e
scoreboard players operation #run_all.v_a e2e /= #c_2 e2e
scoreboard players operation #run_all.v_a e2e %= #c_7 e2e
scoreboard players operation #g.r_compound e2e = #run_all.v_a e2e
scoreboard players set #run_all.v_x_2 e2e 3
scoreboard players set #run_all.v_y e2e 7
scoreboard players set #run_all.v_l e2e 0
scoreboard players set #run_all.t_0 e2e 0
execute if score #run_all.v_x_2 e2e matches ..4 if score #run_all.v_y e2e matches 6.. run scoreboard players set #run_all.t_0 e2e 1
execute if score #run_all.v_x_2 e2e matches 100 run scoreboard players set #run_all.t_0 e2e 1
execute if score #run_all.t_0 e2e matches 1 run function e2e:run_all/b22
scoreboard players set #run_all.t_0 e2e 0
execute unless score #run_all.v_x_2 e2e matches ..4 run scoreboard players set #run_all.t_0 e2e 1
execute unless score #run_all.v_y e2e matches 6.. run scoreboard players set #run_all.t_0 e2e 1
execute if score #run_all.t_0 e2e matches 1 run function e2e:run_all/b23
scoreboard players set #run_all.t_0 e2e 0
execute unless score #run_all.v_x_2 e2e matches 3 run scoreboard players set #run_all.t_0 e2e 1
execute unless score #run_all.v_y e2e matches 7 run scoreboard players set #run_all.t_0 e2e 1
execute if score #run_all.t_0 e2e matches 1 run function e2e:run_all/b24
scoreboard players operation #g.r_logic e2e = #run_all.v_l e2e
scoreboard players set #alice pts 5
scoreboard players add #alice pts 3
execute store result score #run_all.t_0 e2e run scoreboard players get #alice pts
scoreboard players operation #run_all.t_0 e2e *= #c_2 e2e
scoreboard players operation #bob pts = #run_all.t_0 e2e
execute store result score #run_all.t_0 e2e run scoreboard players get #alice pts
execute store result score #run_all.t_1 e2e run scoreboard players get #bob pts
scoreboard players operation #g.r_score e2e = #run_all.t_0 e2e
scoreboard players operation #g.r_score e2e += #run_all.t_1 e2e
scoreboard players set #mathx__make_pair.v_a e2e 3
scoreboard players set #mathx__make_pair.v_b e2e 4
function e2e:mathx__make_pair
data modify storage e2e:vars run_all.t_0 set from storage e2e:ret v
data remove storage e2e:ret v
data modify storage e2e:vars run_all.v_tmp set from storage e2e:vars run_all.t_0
data remove storage e2e:vars run_all.t_0
execute store result score #g.r_struct_ret e2e run data get storage e2e:vars run_all.v_tmp.b
scoreboard players set #mathx__make_pair.v_a e2e 5
scoreboard players set #mathx__make_pair.v_b e2e 6
function e2e:mathx__make_pair
data modify storage e2e:vars run_all.t_0 set from storage e2e:ret v
data remove storage e2e:ret v
data modify storage e2e:vars run_all.v_pr set from storage e2e:vars run_all.t_0
data remove storage e2e:vars run_all.t_0
execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_pr.a
scoreboard players operation #run_all.t_0 e2e *= #c_10 e2e
execute store result score #run_all.t_1 e2e run data get storage e2e:vars run_all.v_pr.b
scoreboard players operation #run_all.t_0 e2e += #run_all.t_1 e2e
scoreboard players set #mathx__square.v_x e2e 2
function e2e:mathx__square
scoreboard players operation #run_all.t_1 e2e = #ret e2e
scoreboard players operation #g.r_pair e2e = #run_all.t_0 e2e
scoreboard players operation #g.r_pair e2e += #run_all.t_1 e2e
scoreboard players set #run_all.v_base e2e 10
data modify storage e2e:vars run_all.v_tmp_2 set value []
execute store result storage e2e:vars run_all.t_0 int 1 run scoreboard players get #run_all.v_base e2e
data modify storage e2e:vars run_all.v_tmp_2 append from storage e2e:vars run_all.t_0
data remove storage e2e:vars run_all.t_0
scoreboard players operation #run_all.t_0 e2e = #run_all.v_base e2e
scoreboard players add #run_all.t_0 e2e 1
execute store result storage e2e:vars run_all.t_0 int 1 run scoreboard players get #run_all.t_0 e2e
data modify storage e2e:vars run_all.v_tmp_2 append from storage e2e:vars run_all.t_0
data remove storage e2e:vars run_all.t_0
scoreboard players operation #run_all.t_0 e2e = #run_all.v_base e2e
scoreboard players operation #run_all.t_0 e2e *= #c_2 e2e
execute store result storage e2e:vars run_all.t_0 int 1 run scoreboard players get #run_all.t_0 e2e
data modify storage e2e:vars run_all.v_tmp_2 append from storage e2e:vars run_all.t_0
data remove storage e2e:vars run_all.t_0
data modify storage e2e:vars run_all.v_xs set from storage e2e:vars run_all.v_tmp_2
data modify storage e2e:vars run_all.v_xs append value 99
execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_xs[0]
execute store result score #run_all.t_1 e2e run data get storage e2e:vars run_all.v_xs[1]
scoreboard players operation #run_all.t_0 e2e += #run_all.t_1 e2e
execute store result score #run_all.t_1 e2e run data get storage e2e:vars run_all.v_xs[2]
scoreboard players operation #run_all.t_0 e2e += #run_all.t_1 e2e
execute store result score #run_all.t_1 e2e run data get storage e2e:vars run_all.v_xs[3]
scoreboard players operation #g.r_list_sum e2e = #run_all.t_0 e2e
scoreboard players operation #g.r_list_sum e2e += #run_all.t_1 e2e
execute store result score #g.r_len e2e run data get storage e2e:vars run_all.v_xs
data modify storage e2e:vars run_all.v_words set value ["x","yy"]
execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_words
scoreboard players operation #run_all.t_0 e2e *= #c_100 e2e
execute store result score #run_all.t_1 e2e run data get storage e2e:globals names
scoreboard players operation #g.r_dynlist e2e = #run_all.t_0 e2e
scoreboard players operation #g.r_dynlist e2e += #run_all.t_1 e2e
data modify storage e2e:vars echo.v_s set value "hello"
function e2e:echo
data modify storage e2e:vars run_all.t_0 set from storage e2e:ret v
data remove storage e2e:ret v
data modify storage e2e:globals greeting set from storage e2e:vars run_all.t_0
data remove storage e2e:vars run_all.t_0
data modify storage e2e:globals names append from storage e2e:globals greeting
data modify storage e2e:globals names append value "world"
scoreboard players set #count_chars.v_n e2e 3
data modify storage e2e:vars count_chars.v_label set value "ab"
function e2e:count_chars
scoreboard players operation #g.r_recur_data e2e = #ret e2e
scoreboard players set #run_all.v_sh e2e 1
scoreboard players operation #run_all.v_sh_2 e2e = #run_all.v_sh e2e
scoreboard players add #run_all.v_sh_2 e2e 1
scoreboard players operation #g.r_shadow e2e = #run_all.v_sh_2 e2e
scoreboard players operation #run_all.t_0 e2e = #g.r_shadow e2e
scoreboard players operation #run_all.t_0 e2e *= #c_10 e2e
scoreboard players operation #g.r_shadow e2e = #run_all.t_0 e2e
scoreboard players operation #g.r_shadow e2e += #run_all.v_sh e2e
data modify storage e2e:vars run_all.v_ys set value [10,20,30,40]
scoreboard players set #run_all.v_ix e2e 2
execute store result storage e2e:macro h0 int 1 run scoreboard players get #run_all.v_ix e2e
function e2e:run_all/m0 with storage e2e:macro
data remove storage e2e:macro h0
scoreboard players operation #run_all.v_hv e2e = #run_all.v_ix e2e
scoreboard players remove #run_all.v_hv e2e 1
execute store result storage e2e:macro h0 int 1 run scoreboard players get #run_all.v_hv e2e
function e2e:run_all/m1 with storage e2e:macro
data remove storage e2e:macro h0
execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_tmp_3
scoreboard players add #run_all.t_0 e2e 5
execute store result storage e2e:vars run_all.t_0 int 1 run scoreboard players get #run_all.t_0 e2e
execute store result storage e2e:macro h0 int 1 run scoreboard players get #run_all.v_hv e2e
function e2e:run_all/m2 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:vars run_all.t_0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #run_all.v_ix e2e
function e2e:run_all/m3 with storage e2e:macro
data remove storage e2e:macro h0
scoreboard players operation #run_all.v_hv_2 e2e = #run_all.v_ix e2e
scoreboard players remove #run_all.v_hv_2 e2e 1
execute store result storage e2e:macro h0 int 1 run scoreboard players get #run_all.v_hv_2 e2e
function e2e:run_all/m4 with storage e2e:macro
data remove storage e2e:macro h0
scoreboard players operation #run_all.v_hv_3 e2e = #run_all.v_ix e2e
scoreboard players add #run_all.v_hv_3 e2e 1
execute store result storage e2e:macro h0 int 1 run scoreboard players get #run_all.v_hv_3 e2e
function e2e:run_all/m5 with storage e2e:macro
data remove storage e2e:macro h0
execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_tmp_4
execute store result score #run_all.t_1 e2e run data get storage e2e:vars run_all.v_tmp_5
scoreboard players operation #run_all.t_1 e2e *= #c_1000 e2e
scoreboard players operation #run_all.t_0 e2e += #run_all.t_1 e2e
execute store result score #run_all.t_1 e2e run data get storage e2e:vars run_all.v_tmp_6
scoreboard players operation #run_all.t_1 e2e *= #c_1000000 e2e
scoreboard players operation #g.r_dyn_index e2e = #run_all.t_0 e2e
scoreboard players operation #g.r_dyn_index e2e += #run_all.t_1 e2e
data modify storage e2e:vars run_all.v_tmp_7 set value []
scoreboard players set #mathx__make_pair.v_a e2e 1
scoreboard players set #mathx__make_pair.v_b e2e 2
function e2e:mathx__make_pair
data modify storage e2e:vars run_all.t_0 set from storage e2e:ret v
data remove storage e2e:ret v
data modify storage e2e:vars run_all.v_tmp_7 append from storage e2e:vars run_all.t_0
data remove storage e2e:vars run_all.t_0
scoreboard players set #mathx__make_pair.v_a e2e 3
scoreboard players set #mathx__make_pair.v_b e2e 4
function e2e:mathx__make_pair
data modify storage e2e:vars run_all.t_0 set from storage e2e:ret v
data remove storage e2e:ret v
data modify storage e2e:vars run_all.v_tmp_7 append from storage e2e:vars run_all.t_0
data remove storage e2e:vars run_all.t_0
data modify storage e2e:vars run_all.v_ps set from storage e2e:vars run_all.v_tmp_7
scoreboard players set #run_all.v_j e2e 1
execute store result storage e2e:macro h0 int 1 run scoreboard players get #run_all.v_j e2e
function e2e:run_all/m6 with storage e2e:macro
data remove storage e2e:macro h0
execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_tmp_8
scoreboard players add #run_all.t_0 e2e 10
execute store result storage e2e:vars run_all.t_0 int 1 run scoreboard players get #run_all.t_0 e2e
execute store result storage e2e:macro h0 int 1 run scoreboard players get #run_all.v_j e2e
function e2e:run_all/m7 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:vars run_all.t_0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #run_all.v_j e2e
function e2e:run_all/m8 with storage e2e:macro
data remove storage e2e:macro h0
execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_tmp_9
scoreboard players operation #run_all.t_0 e2e *= #c_100 e2e
execute store result score #run_all.t_1 e2e run data get storage e2e:vars run_all.v_ps[0].b
scoreboard players operation #g.r_dyn_struct e2e = #run_all.t_0 e2e
scoreboard players operation #g.r_dyn_struct e2e += #run_all.t_1 e2e
data modify storage e2e:vars run_all.v_zs set value [1,2,3,4,5]
scoreboard players set #run_all.v_zk e2e 0
scoreboard players set #run_all.v_zt e2e 0
function e2e:run_all/b25
scoreboard players operation #g.r_dyn_loop e2e = #run_all.v_zt e2e
scoreboard players set #run_all.v_neg e2e -1
execute store result storage e2e:macro h0 int 1 run scoreboard players get #run_all.v_neg e2e
function e2e:run_all/m10 with storage e2e:macro
data remove storage e2e:macro h0
execute store result score #g.r_dyn_neg e2e run data get storage e2e:vars run_all.v_tmp_11
execute store result storage e2e:macro h0 int 1 run scoreboard players get #run_all.v_ix e2e
function e2e:run_all/m11 with storage e2e:macro
data remove storage e2e:macro h0
execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_pr.a
execute store result score #run_all.t_1 e2e run data get storage e2e:vars run_all.v_pr.b
execute store result score #run_all.t_2 e2e run data get storage e2e:vars run_all.v_tmp_12
scoreboard players add #run_all.t_2 e2e 1
tellraw @a {text:"",extra:["fib=",{score:{name:"#g.r_fib",objective:"e2e"}}," greeting=",{nbt:"greeting",storage:"e2e:globals"}," names=",{nbt:"names",storage:"e2e:globals"}," pair=",{score:{name:"#run_all.t_0",objective:"e2e"}},",",{score:{name:"#run_all.t_1",objective:"e2e"}}," sum=",{score:{name:"#run_all.t_2",objective:"e2e"}}," {braces}"]}
function e2e:game__arena__reset
function e2e:game__arena__advance
function e2e:game__arena__advance
scoreboard players operation #run_all.t_0 e2e = #g.game__arena__seconds e2e
scoreboard players operation #run_all.t_0 e2e *= #c_100 e2e
function e2e:game__arena__round_number
scoreboard players operation #run_all.t_1 e2e = #ret e2e
scoreboard players operation #g.r_arena e2e = #run_all.t_0 e2e
scoreboard players operation #g.r_arena e2e += #run_all.t_1 e2e
function e2e:game__arena__scores_len
scoreboard players operation #g.r_arena_len e2e = #ret e2e
function e2e:run_all/_drop
