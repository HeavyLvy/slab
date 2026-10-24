scoreboard players set #run_all.t_0 e2e 0
execute store success score #run_all.t_0 e2e if score #run_all.v_brk_2 e2e matches 0 if score #run_all.v_k e2e < #run_all.v_end e2e
execute unless score #run_all.t_0 e2e matches 1 run return 0
scoreboard players set #run_all.v_cont_2 e2e 0
execute if score #run_all.v_k e2e matches 3 run function e2e:run_all/b8
execute if score #run_all.v_cont_2 e2e matches 0 run function e2e:run_all/b9
scoreboard players add #run_all.v_k e2e 1
return run function e2e:run_all/b7
