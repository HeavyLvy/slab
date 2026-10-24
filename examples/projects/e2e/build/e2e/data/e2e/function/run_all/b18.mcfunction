scoreboard players set #run_all.t_0 e2e 0
execute store success score #run_all.t_0 e2e if score #run_all.v_brk_4 e2e matches 0 if score #run_all.v_b e2e < #run_all.v_end_4 e2e
execute unless score #run_all.t_0 e2e matches 1 run return 0
execute if score #run_all.v_b e2e matches 2 run function e2e:run_all/b19
execute if score #run_all.v_brk_4 e2e matches 0 run function e2e:run_all/b20
scoreboard players add #run_all.v_b e2e 1
return run function e2e:run_all/b18
