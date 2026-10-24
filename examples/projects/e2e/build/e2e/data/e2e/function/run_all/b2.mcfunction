scoreboard players operation #run_all.t_0 e2e = #run_all.v_i e2e
scoreboard players operation #run_all.t_0 e2e %= #c_2 e2e
execute if score #run_all.t_0 e2e matches 0 run function e2e:run_all/b3
execute if score #run_all.v_cont e2e matches 0 run function e2e:run_all/b4
