execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_iter_2
scoreboard players set #run_all.t_1 e2e 0
execute store success score #run_all.t_1 e2e if score #run_all.v_brk_3 e2e matches 0 if score #run_all.t_0 e2e matches 1..
execute unless score #run_all.t_1 e2e matches 1 run return 0
data modify storage e2e:vars run_all.v_name set from storage e2e:vars run_all.v_iter_2[0]
data remove storage e2e:vars run_all.v_iter_2[0]
execute store result score #run_all.t_0 e2e run data get storage e2e:vars run_all.v_name
execute if score #run_all.t_0 e2e matches 3.. run function e2e:run_all/b15
execute if score #run_all.v_cnt e2e matches 2 run function e2e:run_all/b16
return run function e2e:run_all/b14
