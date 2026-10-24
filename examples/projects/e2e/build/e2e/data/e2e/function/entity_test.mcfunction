scoreboard players set #new_id e2e 0
execute positioned 8 64 8 summon minecraft:marker run function e2e:_new_entity
scoreboard players operation #entity_test.v_a e2e = #new_id e2e
scoreboard players set #new_id e2e 0
execute positioned 9 64 9 summon minecraft:marker run function e2e:_new_entity
scoreboard players operation #entity_test.v_b e2e = #new_id e2e
scoreboard players set #entity_test.t_0 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_a e2e
execute store success score #entity_test.t_0 e2e run function e2e:entity_test/m0 with storage e2e:macro
data remove storage e2e:macro h0
scoreboard players set #entity_test.t_1 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_b e2e
execute store success score #entity_test.t_1 e2e run function e2e:entity_test/m1 with storage e2e:macro
data remove storage e2e:macro h0
execute unless score #entity_test.v_a e2e = #entity_test.v_b e2e if score #entity_test.t_0 e2e matches 1 if score #entity_test.t_1 e2e matches 1 run function e2e:entity_test/b0
scoreboard players operation #raise.v_e e2e = #entity_test.v_a e2e
scoreboard players set #raise.v_y e2e 90
function e2e:raise
scoreboard players set #entity_test.v_up e2e 0
execute as @e[type=minecraft:marker,x=8.5,y=90,z=8.5,distance=..0.5] run function e2e:entity_test/b1
scoreboard players operation #g.r_ent_alive e2e = #entity_test.v_up e2e
scoreboard players set #new_id e2e 0
execute as @e[type=minecraft:marker,x=8.5,y=90,z=8.5,distance=..0.5,limit=1] run function e2e:_ensure_id
scoreboard players operation #entity_test.v_again e2e = #new_id e2e
execute if score #entity_test.v_again e2e = #entity_test.v_a e2e run function e2e:entity_test/b2
scoreboard players set #spawn_at.v_x e2e 12
function e2e:spawn_at
scoreboard players operation #entity_test.v_c e2e = #ret e2e
data modify storage e2e:vars entity_test.v_tmp set value []
execute store result storage e2e:vars entity_test.t_0 int 1 run scoreboard players get #entity_test.v_a e2e
data modify storage e2e:vars entity_test.v_tmp append from storage e2e:vars entity_test.t_0
data remove storage e2e:vars entity_test.t_0
execute store result storage e2e:vars entity_test.t_0 int 1 run scoreboard players get #entity_test.v_b e2e
data modify storage e2e:vars entity_test.v_tmp append from storage e2e:vars entity_test.t_0
data remove storage e2e:vars entity_test.t_0
execute store result storage e2e:vars entity_test.t_0 int 1 run scoreboard players get #entity_test.v_c e2e
data modify storage e2e:vars entity_test.v_tmp append from storage e2e:vars entity_test.t_0
data remove storage e2e:vars entity_test.t_0
data modify storage e2e:vars entity_test.v_hs set from storage e2e:vars entity_test.v_tmp
scoreboard players set #entity_test.v_n e2e 0
data modify storage e2e:vars entity_test.v_iter set from storage e2e:vars entity_test.v_hs
function e2e:entity_test/b3
scoreboard players operation #g.r_ent_list e2e = #entity_test.v_n e2e
data modify storage e2e:vars entity_test.v_tmp_2 set value {}
execute store result storage e2e:vars entity_test.t_0 int 1 run scoreboard players get #entity_test.v_a e2e
data modify storage e2e:vars entity_test.v_tmp_2.left set from storage e2e:vars entity_test.t_0
data remove storage e2e:vars entity_test.t_0
execute store result storage e2e:vars entity_test.t_0 int 1 run scoreboard players get #entity_test.v_c e2e
data modify storage e2e:vars entity_test.v_tmp_2.right set from storage e2e:vars entity_test.t_0
data remove storage e2e:vars entity_test.t_0
data modify storage e2e:vars entity_test.v_duel set from storage e2e:vars entity_test.v_tmp_2
scoreboard players set #g.r_ent_struct e2e 0
execute store result score #entity_test.v_hv e2e run data get storage e2e:vars entity_test.v_duel.left
execute store result score #entity_test.v_hv_2 e2e run data get storage e2e:vars entity_test.v_duel.right
scoreboard players set #entity_test.t_0 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_hv e2e
execute store success score #entity_test.t_0 e2e run function e2e:entity_test/m3 with storage e2e:macro
data remove storage e2e:macro h0
scoreboard players set #entity_test.t_1 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_hv_2 e2e
execute store success score #entity_test.t_1 e2e run function e2e:entity_test/m4 with storage e2e:macro
data remove storage e2e:macro h0
execute store result score #entity_test.t_2 e2e run data get storage e2e:vars entity_test.v_duel.left
execute store result score #entity_test.t_3 e2e run data get storage e2e:vars entity_test.v_duel.right
execute if score #entity_test.t_0 e2e matches 1 if score #entity_test.t_1 e2e matches 1 unless score #entity_test.t_2 e2e = #entity_test.t_3 e2e run function e2e:entity_test/b5
scoreboard players operation #raise.v_e e2e = #entity_test.v_c e2e
scoreboard players set #raise.v_y e2e 70
function e2e:raise
scoreboard players operation #entity_test.t_0 e2e = #ret e2e
execute if score #entity_test.t_0 e2e matches 1 run function e2e:entity_test/b6
scoreboard players set #entity_test.v_seen e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_b e2e
function e2e:entity_test/m5 with storage e2e:macro
data remove storage e2e:macro h0
scoreboard players operation #g.r_ent_ctx e2e = #entity_test.v_seen e2e
scoreboard players set #e pts 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_b e2e
function e2e:entity_test/m6 with storage e2e:macro
data remove storage e2e:macro h0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_b e2e
function e2e:entity_test/m7 with storage e2e:macro
data remove storage e2e:macro h0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_b e2e
function e2e:entity_test/m8 with storage e2e:macro
data remove storage e2e:macro h0
scoreboard players set #new_id e2e 0
execute positioned 10 64 10 summon minecraft:marker run function e2e:_new_entity
scoreboard players operation #entity_test.v_d e2e = #new_id e2e
scoreboard players set #new_id e2e 0
execute positioned 11 64 11 summon minecraft:marker run function e2e:_new_entity
scoreboard players operation #entity_test.v_q e2e = #new_id e2e
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_d e2e
function e2e:entity_test/m9 with storage e2e:macro
data remove storage e2e:macro h0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_q e2e
function e2e:entity_test/m10 with storage e2e:macro
data remove storage e2e:macro h0
scoreboard players set #entity_test.v_n_2 e2e 0
execute as @e[tag=forz] run function e2e:entity_test/b8
scoreboard players operation #g.r_for_ent e2e = #entity_test.v_n_2 e2e
scoreboard players set #entity_test.v_standing e2e 0
execute as @e[type=minecraft:marker,x=0.5,y=90,z=0.5,distance=..0.5] run function e2e:entity_test/b9
scoreboard players operation #g.r_for_moved e2e = #entity_test.v_standing e2e
scoreboard players set #new_id e2e 0
execute as @e[type=minecraft:marker,x=0.5,y=90,z=0.5,distance=..0.5,limit=1] run function e2e:_ensure_id
scoreboard players operation #entity_test.v_first e2e = #new_id e2e
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_first e2e
function e2e:entity_test/m13 with storage e2e:macro
data remove storage e2e:macro h0
execute if score #entity_test.t_0 e2e matches 7 run function e2e:entity_test/b10
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_b e2e
function e2e:entity_test/m14 with storage e2e:macro
data remove storage e2e:macro h0
scoreboard players set #entity_test.t_0 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_b e2e
execute store success score #entity_test.t_0 e2e run function e2e:entity_test/m15 with storage e2e:macro
data remove storage e2e:macro h0
execute unless score #entity_test.t_0 e2e matches 1 run function e2e:entity_test/b11
scoreboard players set #entity_test.t_0 e2e 0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_b e2e
execute store success score #entity_test.t_0 e2e run function e2e:entity_test/m16 with storage e2e:macro
data remove storage e2e:macro h0
execute unless score #entity_test.t_0 e2e matches 1 run function e2e:entity_test/b12
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_a e2e
function e2e:entity_test/m17 with storage e2e:macro
data remove storage e2e:macro h0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_c e2e
function e2e:entity_test/m18 with storage e2e:macro
data remove storage e2e:macro h0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_d e2e
function e2e:entity_test/m19 with storage e2e:macro
data remove storage e2e:macro h0
execute store result storage e2e:macro h0 int 1 run scoreboard players get #entity_test.v_q e2e
function e2e:entity_test/m20 with storage e2e:macro
data remove storage e2e:macro h0
function e2e:entity_test/_drop
