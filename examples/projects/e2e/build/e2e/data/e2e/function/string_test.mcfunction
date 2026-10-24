data modify storage e2e:vars string_test.v_kind set value "mark"
scoreboard players set #string_test.v_n e2e 7
data modify storage e2e:macro h0 set from storage e2e:vars string_test.v_kind
function e2e:string_test/m0 with storage e2e:macro
data remove storage e2e:macro h0
data modify storage e2e:vars string_test.v_s set from storage e2e:vars string_test.t_0
data remove storage e2e:vars string_test.t_0
data modify storage e2e:macro h0 set from storage e2e:vars string_test.v_s
execute store result storage e2e:macro h1 int 1 run scoreboard players get #string_test.v_n e2e
function e2e:string_test/m1 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:macro h1
data modify storage e2e:vars string_test.v_s set from storage e2e:vars string_test.t_0
data remove storage e2e:vars string_test.t_0
data modify storage e2e:globals r_str_cat set from storage e2e:vars string_test.v_s
data modify storage e2e:vars string_test.v_ty set value "marker"
data modify storage e2e:macro h0 set from storage e2e:vars string_test.v_ty
function e2e:string_test/m2 with storage e2e:macro
data remove storage e2e:macro h0
data modify storage e2e:vars string_test.v_hv set from storage e2e:vars string_test.t_0
data remove storage e2e:vars string_test.t_0
scoreboard players set #new_id e2e 0
data modify storage e2e:macro h0 set from storage e2e:vars string_test.v_hv
function e2e:string_test/m3 with storage e2e:macro
data remove storage e2e:macro h0
scoreboard players operation #string_test.v_m e2e = #new_id e2e
execute store result storage e2e:macro h0 int 1 run scoreboard players get #string_test.v_n e2e
function e2e:string_test/m4 with storage e2e:macro
data remove storage e2e:macro h0
data modify storage e2e:vars string_test.v_hv_2 set from storage e2e:vars string_test.t_0
data remove storage e2e:vars string_test.t_0
scoreboard players set #string_test.t_0 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #string_test.v_m e2e
data modify storage e2e:macro h1 set from storage e2e:vars string_test.v_hv_2
execute store success score #string_test.t_0 e2e run function e2e:string_test/m5 with storage e2e:macro
data remove storage e2e:macro h0
data remove storage e2e:macro h1
execute if score #string_test.t_0 e2e matches 1 run function e2e:string_test/b0
scoreboard players set #string_test.v_c e2e 0
execute as @e[type=minecraft:marker,tag=t_7] run function e2e:string_test/b1
scoreboard players operation #g.r_str_tag_n e2e = #string_test.v_c e2e
data modify storage e2e:vars string_test.v_mat set value "emerald"
data modify storage e2e:macro h0 set from storage e2e:vars string_test.v_mat
function e2e:string_test/m6 with storage e2e:macro
data remove storage e2e:macro h0
data modify storage e2e:vars string_test.v_hv_3 set from storage e2e:vars string_test.t_0
data remove storage e2e:vars string_test.t_0
data modify storage e2e:macro h0 set from storage e2e:vars string_test.v_hv_3
function e2e:string_test/m7 with storage e2e:macro
data remove storage e2e:macro h0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #string_test.v_n e2e
function e2e:string_test/m8 with storage e2e:macro
data remove storage e2e:macro h0
data modify storage e2e:vars string_test.v_hv_4 set from storage e2e:vars string_test.t_0
data remove storage e2e:vars string_test.t_0
data modify storage e2e:macro h0 set from storage e2e:vars string_test.v_hv_4
function e2e:string_test/m9 with storage e2e:macro
data remove storage e2e:macro h0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #string_test.v_m e2e
function e2e:string_test/m10 with storage e2e:macro
data remove storage e2e:macro h0
function e2e:string_test/_drop
